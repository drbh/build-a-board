//! The netlist. Every net is a constant, so a misspelt net is a compile error.

use bb_sdk::Net;

bb_sdk::nets! {
    // Supplies.
    /// Battery, before the fuse.
    VBAT_RAW = "VBAT_RAW",
    /// Battery, after the fuse.
    VBAT = "VBAT",
    /// Servo rail, from the servo buck.
    V6 = "6V",
    /// Control rail, from the control buck.
    V5 = "5V",
    /// The ESP32 module's regulator output.
    V3_3 = "3V3",
    GND = "GND",

    // Servo buck internals.
    SW = "SW",
    BST = "BST",
    RT = "RT",
    SS = "SS",
    FB = "FB",
    FB_TOP = "FB_TOP",

    // Control buck internals.
    SW5 = "SW5",
    BST5 = "BST5",
    RT5 = "RT5",
    SS5 = "SS5",
    FB5 = "FB5",
    FB5_TOP = "FB5_TOP",
    FB5_BOTTOM = "FB5_BOTTOM",

    // I2C to the IMU.
    SCL = "SCL",
    SDA = "SDA",

    // Left wheel: SimpleFOC commands, fault, phases, encoder.
    SERVO_L = "SERVO_L",
    IN1_L = "IN1_L",
    IN2_L = "IN2_L",
    IN3_L = "IN3_L",
    EN = "EN",
    NFAULT_L = "NFAULT_L",
    M1_L = "M1_L",
    M2_L = "M2_L",
    M3_L = "M3_L",
    A_L = "A_L",
    B_L = "B_L",
    C_L = "C_L",

    // Right wheel.
    SERVO_R = "SERVO_R",
    IN1_R = "IN1_R",
    IN2_R = "IN2_R",
    IN3_R = "IN3_R",
    EN_R = "EN_R",
    NFAULT_R = "NFAULT_R",
    M1_R = "M1_R",
    M2_R = "M2_R",
    M3_R = "M3_R",
    A_R = "A_R",
    B_R = "B_R",
    C_R = "C_R",

    // ESP32 pins deliberately left open.
    NC1 = "NC1",
    NC2 = "NC2",
    NC3 = "NC3",
    NC12 = "NC12",
    NC15 = "NC15",
    NC_EN = "NC_EN",
}

pub const NO_CONNECT: [Net; 6] = [NC1, NC2, NC3, NC12, NC15, NC_EN];
pub const BATTERY: [Net; 2] = [VBAT_RAW, VBAT];
pub const MOTOR_PHASES: [Net; 6] = [M1_L, M2_L, M3_L, M1_R, M2_R, M3_R];

/// The ESP32 dev module's two 15-pin rows, pin 1 first.
pub const ESP_LEFT: [Net; 15] =
    [V5, GND, SERVO_L, NC12, SCL, SDA, IN1_L, IN2_L, IN3_L, EN, A_L, B_L, NFAULT_L, C_L, NC_EN];
pub const ESP_RIGHT: [Net; 15] =
    [V3_3, GND, NC15, NC2, SERVO_R, A_R, B_R, NFAULT_R, C_R, IN1_R, IN2_R, NC3, NC1, IN3_R, EN_R];

/// One wheel's nets: its SimpleFOC Mini, motor, and encoder.
#[derive(Clone, Copy, Debug)]
pub struct Wheel {
    /// Silkscreen name: `LEFT` or `RIGHT`.
    pub name: &'static str,
    /// Short name: `L` or `R`.
    pub letter: &'static str,
    pub in1: Net,
    pub in2: Net,
    pub in3: Net,
    pub enable: Net,
    pub nfault: Net,
    pub m1: Net,
    pub m2: Net,
    pub m3: Net,
    pub a: Net,
    pub b: Net,
    pub c: Net,
}

pub const LEFT: Wheel = Wheel {
    name: "LEFT",
    letter: "L",
    in1: IN1_L,
    in2: IN2_L,
    in3: IN3_L,
    enable: EN,
    nfault: NFAULT_L,
    m1: M1_L,
    m2: M2_L,
    m3: M3_L,
    a: A_L,
    b: B_L,
    c: C_L,
};

pub const RIGHT: Wheel = Wheel {
    name: "RIGHT",
    letter: "R",
    in1: IN1_R,
    in2: IN2_R,
    in3: IN3_R,
    enable: EN_R,
    nfault: NFAULT_R,
    m1: M1_R,
    m2: M2_R,
    m3: M3_R,
    a: A_R,
    b: B_R,
    c: C_R,
};
