//! Run a board component in a sandbox.
//!
//! The component gets WASI with nothing behind it: no filesystem, no
//! environment, no network, a frozen clock, and fixed "random" bytes. With
//! NaN canonicalization on, the same component always yields the same design.

use std::time::Duration;

use anyhow::{Result, anyhow};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::clocks::{HostMonotonicClock, HostWallClock};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "wit",
    world: "board",
});

pub use bb::board::types as wit;

/// Upper bound on guest work, so a runaway board fails instead of hanging.
const FUEL: u64 = 50_000_000_000;

struct State {
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

/// Time stands still inside a board.
struct Frozen;

impl HostWallClock for Frozen {
    fn resolution(&self) -> Duration {
        Duration::from_secs(1)
    }

    fn now(&self) -> Duration {
        Duration::ZERO
    }
}

impl HostMonotonicClock for Frozen {
    fn resolution(&self) -> u64 {
        1
    }

    fn now(&self) -> u64 {
        0
    }
}

/// Wasmtime has its own error type; carry its full message into ours.
fn wt<T>(result: wasmtime::Result<T>, what: &str) -> Result<T> {
    result.map_err(|err| anyhow!("{what}: {err:?}"))
}

/// A loaded, instantiated board component.
pub struct Guest {
    store: Store<State>,
    board: Board,
}

impl Guest {
    pub fn load(wasm: &[u8]) -> Result<Self> {
        let mut config = Config::new();
        config.cranelift_nan_canonicalization(true).consume_fuel(true);
        let engine = wt(Engine::new(&config), "starting the WebAssembly engine")?;
        let component = wt(Component::new(&engine, wasm), "loading the board component")?;
        let mut linker = Linker::new(&engine);
        wt(wasmtime_wasi::p2::add_to_linker_sync(&mut linker), "providing WASI")?;
        let wasi = WasiCtx::builder()
            .inherit_stderr()
            .wall_clock(Frozen)
            .monotonic_clock(Frozen)
            .secure_random(wasmtime_wasi::Deterministic::new(vec![0x42]))
            .insecure_random(wasmtime_wasi::Deterministic::new(vec![0x42]))
            .insecure_random_seed(0)
            .build();
        let mut store = Store::new(&engine, State { wasi, table: ResourceTable::new() });
        wt(store.set_fuel(FUEL), "setting the fuel budget")?;
        let board = wt(Board::instantiate(&mut store, &component, &linker), "instantiating the board")?;
        Ok(Self { store, board })
    }

    pub fn build(&mut self) -> Result<wit::Design> {
        self.board
            .call_build(&mut self.store)
            .map_err(|trap| failed("the board program", &trap))?
            .map_err(|message| anyhow!("{message}"))
    }

    pub fn route_width(&mut self, track: &wit::RoutedTrack) -> Result<Option<f64>> {
        self.board.call_route_width(&mut self.store, track).map_err(|trap| failed("route-width", &trap))
    }
}

/// A trap as one line: Rust panics print their own message (stderr is
/// inherited), so the wasm backtrace adds nothing for a board author.
fn failed(what: &str, trap: &wasmtime::Error) -> anyhow::Error {
    match trap.downcast_ref::<wasmtime::Trap>() {
        Some(wasmtime::Trap::OutOfFuel) => anyhow!("{what} ran out of fuel (an endless loop?)"),
        Some(wasmtime::Trap::UnreachableCodeReached) => anyhow!("{what} panicked (its message is above)"),
        Some(code) => anyhow!("{what} trapped: {code}"),
        None => anyhow!("{what} failed: {trap:#}"),
    }
}
