//! Fixed routes between blocks. FreeRouting completes only short escapes.
//!
//! Every pad is named through a block's handles, so a route can only reach
//! parts that exist, by pins that exist.

use bb_sdk::*;

use crate::Devboard;
use crate::nets::*;

/// The common signal hop: 0.2 mm tracks and 0.6/0.3 mm vias.
fn signal(net: Net) -> Route {
    route(net).width(0.2).via_size(0.6, 0.3)
}

impl Devboard {
    /// Supply wiring between blocks.
    pub fn power_routes(&self) -> Vec<Item> {
        let Self { esp, probes, servo_buck, control_buck, power, drives, .. } = self;
        let c1 = &servo_buck.input[0];
        let c7 = &power.battery_bulk;
        let [c6, c8] = &power.servo_bulk;
        items![
            track(V5, [probes.v5.point(), at(60.6, 86.095)]).width(0.5),
            // 5 V to the ESP32 socket on B.Cu, around the antenna keepout.
            via(V5, (60.6, 86.095)).size(0.8, 0.4),
            track(V5, [at(60.6, 86.095), at(58, 82), at(58, 40), esp.left.on(V5)]).width(0.5).layer(Layer::BCu),
            track(
                VBAT,
                [
                    control_buck.input[0].p1(),
                    at(38.95, 80),
                    at(32, 80),
                    at(32, 100),
                    at(54.16, 100),
                    power.tvs.cathode()
                ]
            )
            .width(1.5),
            track(VBAT, [c7.positive(), at(33.5, 78), at(38.95, 80)]).width(1.5),
            // Mini supply branches avoid the encoder and control pads.
            track(VBAT, [drives.left.power.vbat(), at(33.8095, 62), at(46, 65), at(46, 72), at(36, 72), c7.positive()])
                .width(1.5),
            track(VBAT, [drives.right.power.vbat(), at(108, 64), at(108, 80)]).width(1.5),
            via(VBAT, c1.p1()),
            via(VBAT, (68, 80)),
            track(VBAT, [c1.p1(), at(68, 80)]).width(1.0).layer(Layer::BCu),
            // The servo buck's VBAT lead stops 3.5 mm short of C1; the router
            // closes the gap, and the segment must then stay 1 mm wide.
            track(VBAT, [c1.p1() + (3.5, 0), c1.p1()]).width(1.0).keep_routed(),
            // The right-servo signal splits the front VBAT pour; stitch both sides on B.Cu.
            via(VBAT, (99, 80)),
            via(VBAT, (103, 80)),
            track(VBAT, [at(99, 80), at(103, 80)]).width(1.5).layer(Layer::BCu),
            // 6 V runs unbroken from L1 to the servo headers along the clear
            // y=101 corridor. (A 5 A fuse behind a 3.5 A regulator could never open.)
            track(V6, [at(93.2, 91.7), at(93.2, 101), at(76.4, 101), at(76.4, 107)]).width(2.0),
            track(V6, [at(76.4, 107), at(113.54, 107), drives.servo_right.on(V6)]).width(2.0),
            track(V6, [at(86.5, 107), c6.positive()]).width(2.0),
            track(V6, [at(105.54, 107), drives.servo_left.on(V6)]).width(2.0),
            // C8 joins the bus on F.Cu; two local ground vias keep its pulse return short.
            track(V6, [c8.positive(), at(105, 91), at(105, 107)]).width(2.0),
            track(GND, [c8.negative(), at(113, 93), at(112, 93)]).width(1.5),
            track(GND, [at(113, 93), at(114, 93)]).width(1.5),
            via(GND, (112, 93)).size(0.8, 0.4),
            via(GND, (114, 93)).size(0.8, 0.4),
        ]
    }

    pub fn ground_routes(&self) -> Vec<Item> {
        let Self { esp, drives, encoders: [_, encoder_r], .. } = self;
        items![
            // Bridges that keep both back-pour regions on one return near the
            // right edge, where the control fan-out can split them.
            via(GND, (90, 39.54)).size(0.8, 0.4),
            track(GND, [esp.right.on(GND), at(90, 39.54)]).width(0.5).layer(Layer::BCu),
            via(GND, (90, 72)).size(0.8, 0.4),
            track(GND, [at(90, 39.54), at(90, 72)]).width(1.0),
            via(GND, (90, 80)).size(0.8, 0.4),
            track(GND, [at(90, 72), at(90, 80)]).width(0.6),
            track(GND, [at(90, 80), at(90, 82), at(102, 82), at(102, 86.5), at(99.3, 86.5)])
                .width(0.6)
                .layer(Layer::BCu),
            via(GND, (50, 39.54)).size(0.8, 0.4),
            track(GND, [esp.left.on(GND), at(50, 39.54)]).width(0.5),
            // Upper-right ground trunk, hopping layers around the M1_R route.
            via(GND, (119.0, 53.0)).size(0.8, 0.4),
            track(GND, [at(119.0, 53.0), at(117.0, 56.5)]).width(0.6),
            via(GND, (117.0, 56.5)).size(0.8, 0.4),
            track(
                GND,
                [
                    at(117.0, 56.5),
                    drives.right.power.gnd(),
                    at(114, 64),
                    at(114, 70),
                    at(94.0, 70),
                    encoder_r.header.on(GND)
                ]
            )
            .width(0.6)
            .layer(Layer::BCu),
        ]
    }

    pub fn signal_routes(&self) -> Vec<Item> {
        let Self { esp, i2c, pulldowns, probes, drives, encoders: [_, encoder_r], .. } = self;
        let (j8, j13) = (&esp.right, &encoder_r.header);
        let (phases, motor) = (&drives.right.phase, &drives.motor_right);
        items![
            // Right-wheel phases follow the corridor that routed successfully for M3.
            track(M1_R, [phases.m1(), at(106.0, 54.572), at(119.0, 54.572), at(119.0, 78.0), motor.on(M1_R)])
                .width(0.5)
                .layer(Layer::BCu),
            track(
                M2_R,
                [
                    phases.m2(),
                    at(99.0, 48.0315),
                    at(106.0, 48.0315),
                    at(118.0, 60.0315),
                    at(118.0, 76.0),
                    motor.on(M2_R)
                ]
            )
            .width(0.5),
            track(M3_R, [phases.m3(), at(99.0212, 50.5715), at(105.7732, 50.5715), at(116.0, 60.7983), motor.on(M3_R)])
                .width(0.5),
            // The right encoder's A/B/C leave J8 stacked and arrive at J13
            // reversed. F.Cu is walled off by the GND trunk at x=90 and IN3_R at
            // x=91, so all three cross on B.Cu, each on its own corridor.
            signal(A_R)
                .to(j8.on(A_R))
                .to(at(87.5431, 50.8431))
                .to(at(88.9, 50.8431))
                .via(Layer::BCu)
                .to(at(89.8, 50.8431))
                .to(at(89.8, 57.5))
                .to(at(96.5, 57.5))
                .via(Layer::FCu)
                .to(j13.on(A_R)),
            signal(B_R)
                .to(j8.on(B_R))
                .to(at(87.2998, 52.24))
                .to(at(88.85, 53.7902))
                .via(Layer::BCu)
                .to(at(88.85, 59.5))
                .to(at(99.0, 59.5))
                .via(Layer::FCu)
                .to(j13.on(B_R)),
            // C_R owns the outermost pad, so it crosses A_R and B_R on F.Cu instead.
            signal(C_R)
                .to(j8.on(C_R))
                .to(at(88.7563, 59.6763))
                .to(at(88.7563, 60.1184))
                .via(Layer::BCu)
                .to(at(88.7563, 62.5))
                .to(at(101.5, 62.5))
                .to(j13.on(C_R)),
            track(
                IN2_R,
                [j8.on(IN2_R), at(84.8034, 60.8034), at(84.8034, 40.8), at(93.3026, 40.8), at(94.8235, 44.9489)]
            )
            .width(0.2)
            .layer(Layer::BCu),
            via(IN2_R, (94.8235, 44.9489)).size(0.6, 0.3),
            track(
                IN1_R,
                [
                    j8.on(IN1_R),
                    at(85.1614, 58.6213),
                    at(83.84, 57.3),
                    at(83.84, 35.4157),
                    at(85.3715, 33.8842),
                    at(89.0746, 33.8842),
                    at(89.9763, 32.9825),
                    at(93.541, 32.9825),
                    at(94.4427, 33.8842),
                    at(95.7748, 33.8842),
                    at(100.9453, 39.0547),
                ]
            )
            .width(0.2),
            via(IN1_R, (100.9453, 39.0547)).size(0.6, 0.3),
            signal(IN1_R)
                .to(at(85.1614, 58.6213))
                .to(at(83.4492, 60.3335))
                .via(Layer::BCu)
                .to(at(83.4492, 62.8072))
                .to(at(80.997, 65.2594))
                .to(at(65.7612, 65.2594))
                .via(Layer::FCu)
                .to(at(66.4345, 65.2595))
                .to(pulldowns.on(IN1_R).p1()),
            track(
                NFAULT_R,
                [
                    j8.on(NFAULT_R),
                    at(84.7417, 53.1217),
                    at(84.7417, 35.7892),
                    at(85.2616, 35.2693),
                    at(89.1569, 35.2693),
                    at(89.1569, 35.2692),
                ]
            )
            .width(0.2),
            via(NFAULT_R, (89.1569, 35.2692)).size(0.6, 0.3),
            track(
                NFAULT_R,
                [
                    at(89.1569, 35.2692),
                    at(90.5419, 33.8842),
                    at(91.4629, 33.8842),
                    at(92.3286, 34.7499),
                    at(92.3286, 51.7088),
                    // Stand off J13 pin 1: on the 2.5 mm XH pitch it sits at x=104.
                    at(106.2, 65.5802),
                    at(106.2, 68.3527),
                    at(103.0799, 71.4728),
                    at(100.9453, 71.4728),
                ]
            )
            .width(0.2),
            via(NFAULT_R, (100.9453, 71.4728)).size(0.6, 0.3),
            track(
                NFAULT_R,
                [i2c.fault_pullups[1].p2(), at(81.5748, 83.5918), at(92.0082, 83.5918), probes.fault_right.point()]
            )
            .width(0.2),
            signal(NFAULT_R)
                .to(probes.fault_right.point())
                .to(at(95.0511, 80.5489))
                .to(at(95.0511, 77.1654))
                .via(Layer::BCu)
                .to(at(100.7437, 71.4728))
                .to(at(100.9453, 71.4728)),
            track(
                EN_R,
                [
                    pulldowns.on(EN_R).p1(),
                    at(80.9453, 66.1203),
                    at(80.9454, 66.1203),
                    at(80.9454, 67.1054),
                    j8.on(EN_R)
                ]
            )
            .width(0.2),
            track(
                EN_R,
                [
                    j8.on(EN_R),
                    at(85.1111, 73.8489),
                    at(85.1111, 77.8268),
                    at(85.8216, 78.5373),
                    at(97.9427, 78.5373),
                    at(102.0601, 74.4199),
                    at(103.8924, 74.4199),
                ]
            )
            .width(0.2)
            .layer(Layer::BCu),
            via(EN_R, (103.8924, 74.4199)).size(0.6, 0.3),
            // Escape pin 9 outward before changing layers so it cannot cross pin 10.
            track(EN_R, [drives.right.control.en(), at(116.5, 41.9355)]).width(0.2),
            via(EN_R, (116.5, 41.9355)).size(0.6, 0.3),
            via(IN3_R, (91.0, 35.0)).size(0.6, 0.3),
            signal(IN3_R)
                .to(at(91.0, 35.0))
                .to(at(91.0, 69.0))
                .via(Layer::BCu)
                .to(at(89.0, 69.0))
                .via(Layer::FCu)
                .to(j8.on(IN3_R)),
            signal(IN3_R)
                .to(j8.on(IN3_R))
                .to(at(82.6159, 66.2359))
                .to(at(82.6159, 66.2109))
                .via(Layer::BCu)
                .to(at(76.5556, 66.2109))
                .to(at(74.5101, 68.2564))
                .via(Layer::FCu)
                .to(at(75.175, 67.5915))
                .to(pulldowns.on(IN3_R).p1()),
            signal(SDA)
                .to(esp.left.on(SDA))
                .to(at(55.0, 49.7))
                .via(Layer::BCu)
                .to(at(55.0, 74.0))
                .via(Layer::FCu)
                .to(at(60.0, 74.0)),
            signal(SDA)
                .to(at(60.0, 74.0))
                .via(Layer::BCu)
                .to(at(72.9125, 74.0))
                .to(at(72.9125, 76.5))
                .via(Layer::FCu)
                .to(i2c.sda_pullup.p2()),
            // 3V3 down the right edge, alternating layers around the phase routes.
            signal(V3_3)
                .layer(Layer::BCu)
                .to(j8.on(V3_3))
                .to(at(95.0, 37.0))
                .to(at(104.0, 46.0))
                .to(at(104.0, 51.4))
                .via(Layer::FCu)
                .to(at(104.0, 55.8))
                .via(Layer::BCu)
                .to(at(104.0, 65.0))
                .via(Layer::FCu)
                .to(j13.on(V3_3)),
            track(IN2_L, [pulldowns.on(IN2_L).p1(), at(72.0, 60.825), at(72.0, 54.78), esp.left.on(IN2_L)]).width(0.2),
            // Fixed so FreeRouting cannot strand the ESP32 branch after joining
            // the pull-down and servo header.
            track(SERVO_L, [esp.left.on(SERVO_L), at(80.0, 42.08), at(80.0, 60.8), at(67.0, 60.8), at(65.175, 60.625)])
                .width(0.2)
                .layer(Layer::BCu),
            via(SERVO_L, (65.175, 60.625)).size(0.6, 0.3),
            track(SERVO_L, [at(65.175, 60.625), pulldowns.on(SERVO_L).p1()]).width(0.2),
            signal(IN3_L)
                .to(pulldowns.on(IN3_L).p1())
                .to(at(75.0, 59.825))
                .via(Layer::BCu)
                .to(at(75.0, 57.32))
                .to(esp.left.on(IN3_L)),
            // Right servo PWM: away from the edge, under the Mini supply branch
            // on B.Cu, and a short B.Cu hop under C8's wide 6V branch.
            track(SERVO_R, [j8.on(SERVO_R), at(88, 47.16), at(88, 74), at(91, 74)]).width(0.4).layer(Layer::BCu),
            via(SERVO_R, (91, 74)).size(0.8, 0.4),
            track(SERVO_R, [at(91, 74), at(101, 74), at(101, 104), at(102.5, 104)]).width(0.4),
            route(SERVO_R)
                .via_size(0.8, 0.4)
                .to(at(102.5, 104))
                .via(Layer::BCu)
                .to(at(107.5, 104))
                .via(Layer::FCu)
                .to(at(118, 104))
                .to(at(118, 113))
                .to(drives.servo_right.on(SERVO_R)),
        ]
    }
}
