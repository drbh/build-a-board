"""Balance Bot devboard: unified 100 x 100 mm, two-layer power and control board."""

load(
    "parts.star",
    "C0603",
    "C0805",
    "C1206",
    "R0603",
    "ap64352",
    "cya0850",
    "fiducial",
    "fuse_holder",
    "labelled_block",
    "mounting_hole",
    "numbered",
    "simplefoc_mini_v1",
    "testpoint",
    "thermal_vias",
    "two_pin",
    "vchip_bulk",
)

project(
    "balance-bot-devboard",
    lcsc = {
        "AP64352SP-13": "C2071665",
        "CYA0850-10UH": "C19268645",
        "GRM21BR61H106KE43L": "C440198",
        "CL31A226KAHNNNE": "C12891",
        "CC0603KRX7R9BB104": "C14663",
        "0603WAF1203T5E": "C25808",
        "0603WAF1002T5E": "C25804",
        "0603WAF2002T5E": "C4184",
        "0603WAF2003T5E": "C25811",
        "0603WAF4701T5E": "C23162",
        "BXJ25V470M10*10 10.0TP": "C3002464",
    },
    order_quantity = 5,
    checks = ["pincheck", "simcheck", "boardcheck", "jlccheck"],
    release_check = "boardcheck",
    contract = "boardcheck/contract.json",
    release_files = [
        "board.star", "parts.star", "Makefile", "README.md", "requirements.txt", "biggie.png", "jameo.png",
        "bom", "docs", "tools",
    ],
    decisions = {
        "dual_servo_stall": "Accepted: protected 6 V rail may droop and servos may brown out",
        "module_footprints": "Verified against the intended physical modules",
    },
    schematic = {
        "title": "Balance Bot Devboard",
        "company": "Balance Bot devboard",
        "comment": "Generated from routed PCB connectivity",
        "library": "BalanceBot_Devboard",
    },
)

board(
    copper_layers = 2,
    # Values belong in fabrication data, not on the silkscreen. Connector,
    # mechanical, and probe designators give way to purpose-written legends:
    # at a test pad the useful legend is its net, not "TP3".
    hide_references = ["J", "H", "FID", "FC", "FH", "TP"],
    finish = {"mask": "Black", "silk": "White", "copper": "HASL lead-free"},
    # Once routed, the board-local generated parts drop their virtual library
    # link and designators, as on the reviewed release.
    unlink_after_route = "BalanceBot_Devboard",
)

# Mechanical courtyard clashes are reviewed in the assembly render; they are
# warnings. Electrical, spacing, outline, and connectivity checks gate release.
for rule in ("courtyards_overlap", "pth_inside_courtyard", "npth_inside_courtyard"):
    rule_severity(rule, "warning")

ESP_LEFT = [
    "5V", "GND", "SERVO_L", "NC12", "SCL", "SDA", "IN1_L", "IN2_L", "IN3_L", "EN", "A_L", "B_L", "NFAULT_L", "C_L",
    "NC_EN",
]
ESP_RIGHT = [
    "3V3", "GND", "NC15", "NC2", "SERVO_R", "A_R", "B_R", "NFAULT_R", "C_R", "IN1_R", "IN2_R", "NC3", "NC1", "IN3_R",
    "EN_R",
]
MOTOR_PHASES = ["M1_L", "M2_L", "M3_L", "M1_R", "M2_R", "M3_R"]

nets(ESP_LEFT + ESP_RIGHT + MOTOR_PHASES + [
    "GND", "VBAT_RAW", "VBAT", "SW", "BST", "RT", "SS", "FB", "FB_TOP", "SW5", "BST5", "RT5", "SS5", "FB5",
    "FB5_TOP", "FB5_BOTTOM", "6V", "5V", "LED5",
])
netclass("Battery", track_width = 1.5, nets = ["VBAT_RAW", "VBAT"], via_diameter = 1.0, via_drill = 0.5)
netclass("MotorPhase", track_width = 0.5, nets = MOTOR_PHASES)
outline("Edge.Cuts", [(20, 20), (120, 20), (120, 120), (20, 120)], width = 0.1)

# Accessible, unpopulated probe pads. Pad and legend are declared together so
# they cannot drift apart (an earlier revision shipped legends over bare
# mask while three real pads went unlabelled). TP4 and TP5 are the same 6V
# net on purpose: regulator output and the far end of the servo trunk, so a
# stall's droop can be split into regulator sag and trunk IR drop.
TESTPOINTS = [
    # ref, net, pad, legend, legend position
    ("TP6", "5V", (60.6, 82.0), "5V", (60.6, 80.0)),
    ("TP1", "GND", (68.0, 82.0), "GND", (68.0, 80.0)),
    ("TP2", "VBAT_RAW", (71.2, 82.0), "RAW", (71.2, 80.0)),
    ("TP3", "VBAT", (74.4, 82.0), "BAT", (74.4, 80.0)),
    ("TP7", "3V3", (87.2, 82.0), "3V3", (87.2, 80.0)),
    ("TP8", "NFAULT_L", (90.4, 82.0), "FL", (90.4, 80.0)),
    ("TP9", "NFAULT_R", (93.6, 82.0), "FR", (93.6, 80.0)),
    ("TP4", "6V", (93.2, 89.095), "6V SRC", (92.4, 92.6)),
    ("TP5", "6V", (84.0, 107.0), "6V LOAD", (84.0, 105.0)),
]

# --- Control: ESP32 sockets, encoder/IMU headers, pull-ups and pull-downs ---
# The ESP32 antenna points at the top edge; the ground plane has a matching notch.

part("J7", "ESP32 LEFT", "Connector_PinSocket_2.54mm:PinSocket_1x15_P2.54mm_Vertical", at = (61, 37), pins = numbered(ESP_LEFT))
part("J8", "ESP32 RIGHT", "Connector_PinSocket_2.54mm:PinSocket_1x15_P2.54mm_Vertical", at = (86.4, 37), pins = numbered(ESP_RIGHT))

# J10 and J13 share pin order and orientation so one hand-built cable fits
# either side. That forces the right encoder's A/B/C to cross on the way from
# J8 (see the fixed fan-out below).
for ref, x, side in (("J10", 32, "L"), ("J13", 94, "R")):
    part(
        ref,
        "ENCODER %s / JST XH2.5" % side,
        "Connector_JST:JST_XH_B5B-XH-A_1x05_P2.50mm_Vertical",
        at = (x, 67.4),
        pins = {"1": "GND", "2": "A_" + side, "3": "B_" + side, "4": "C_" + side, "5": "3V3"},
    )

two_pin("R10", "4.7k", "Resistor_SMD:R_0603_1608Metric_Pad0.98x0.95mm_HandSolder", (63, 78), "3V3", "SCL")
two_pin("R11", "4.7k", "Resistor_SMD:R_0603_1608Metric_Pad0.98x0.95mm_HandSolder", (72, 78), "3V3", "SDA")
# SimpleFOC nFAULT is open-drain; physical pull-ups keep both fault lines valid in reset.
two_pin("R12", "10k 1%", R0603, (74.5, 78.192), "3V3", "NFAULT_L", angle = 270)
two_pin("R13", "10k 1%", R0603, (77, 78.192), "3V3", "NFAULT_R", angle = 270)
part(
    "J11",
    "IMU / MALE",
    "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical",
    at = (48.19, 78),
    angle = 90,
    pins = {"1": "GND", "2": "SCL", "3": "SDA", "4": "3V3"},
)

# Physical pull-downs make every actuator command deterministic while the
# ESP32 is in reset, independent of the attached module. Each gets its own
# short via to the back ground plane rather than a shared bus between pads.
STARTUP_PULLDOWNS = [
    # ref, signal, position, angle, ground pad x
    ("R14", "SERVO_L", (66, 62), 0, 66.825),
    ("R15", "IN1_L", (70, 62), 0, 70.825),
    ("R16", "IN2_L", (74, 62), 0, 74.825),
    ("R17", "IN3_L", (78, 62), 0, 78.825),
    ("R18", "EN", (82, 62), 180, 81.175),  # GND pad faces the bus, not EN
    ("R19", "IN1_R", (68, 66), 0, 68.825),
    ("R20", "IN2_R", (72, 66), 0, 72.825),
    ("R21", "IN3_R", (76, 66), 0, 76.825),
    ("R22", "EN_R", (80, 66), 180, 79.175),
]
for ref, signal, at, angle, _ in STARTUP_PULLDOWNS:
    two_pin(ref, "10k 1%", R0603, at, signal, "GND", angle = angle)
for _, _, (_, y), _, ground_x in STARTUP_PULLDOWNS:
    track("GND", [(ground_x, y), (ground_x, y + 1.5)], width = 0.3)
    via("GND", (ground_x, y + 1.5), diameter = 0.6, drill = 0.3)

# Local fixed join; avoids version-specific autorouter endpoint misses.
track("3V3", [pin("R12", "1"), (74.5, 75.5), (70.0, 75.5), (70.0, 78.0), (71.175, 78.0)], width = 0.25)

for ref, net, at, _, _ in TESTPOINTS:
    testpoint(ref, net, at)
track("5V", [pin("TP6", "1"), (60.6, 86.095)], width = 0.5)

# Three non-collinear fiducials for SMT alignment.
fiducial("FID1", (30, 26))
fiducial("FID2", (110, 26))
fiducial("FID3", (115, 98))

# --- Power entry and actuators ---

# The battery carries a female XT60, so a male pigtail solders into the AMASS
# holes, rotated vertical for clearance. Pad 1 is the keyed negative contact.
# The upright XT60 body model is not fitted with a pigtail.
part(
    "J1",
    "XT60-M PIGTAIL / 4S INPUT",
    "Connector_AMASS:AMASS_XT60-M_1x02_P7.20mm_Vertical",
    at = (25, 94),
    angle = 90,
    pins = {"1": "GND", "2": "VBAT_RAW"},
    models = [],
)
fuse_holder("FH1", "MINI holder / F1 10A", (43, 111), "VBAT_RAW", "VBAT")
simplefoc_mini_v1("LEFT", (34, 50), 90, control = "J9", power = "J2", phase = "J14")
simplefoc_mini_v1("RIGHT", (106, 50), 90, control = "J12", power = "J3", phase = "J15")

# XH rather than PH on the phases: the Mini is 2 A continuous / 3 A peak per
# phase and PH is rated 2 A per contact; XH is 3 A and takes 22 AWG.
part("J16", "LEFT MOTOR XH2.5", "Connector_JST:JST_XH_B3B-XH-A_1x03_P2.50mm_Vertical", at = (24, 74), angle = 270, pins = numbered(["M1_L", "M2_L", "M3_L"]))
part("J17", "RIGHT MOTOR XH2.5", "Connector_JST:JST_XH_B3B-XH-A_1x03_P2.50mm_Vertical", at = (116, 78), angle = 90, pins = numbered(["M1_R", "M2_R", "M3_R"]))
part("J4", "SERVO L", "Connector_PinHeader_2.54mm:PinHeader_1x03_P2.54mm_Vertical", at = (103, 113), angle = 90, pins = numbered(["GND", "6V", "SERVO_L"]))
part("J5", "SERVO R", "Connector_PinHeader_2.54mm:PinHeader_1x03_P2.54mm_Vertical", at = (111, 113), angle = 90, pins = numbered(["GND", "6V", "SERVO_R"]))
# KiCad's banded pad 1 is the TVS cathode: positive VBAT.
part("D1", "P6KE22A", "Diode_THT:D_DO-15_P10.16mm_Horizontal", at = (54.16, 103), angle = 180, pins = numbered(["VBAT", "GND"]))

# All three bulk caps share one part and feeder line; C6 + C8 give the servo
# rail 940 uF at ~40 mohm.
vchip_bulk("C7", "470u 25V", (38, 76), "VBAT", "SamYoung_BXJ_J10_C3002464")
vchip_bulk("C6", "470u 25V", (91, 111), "6V", "SamYoung_BXJ_J10_C3002464")
vchip_bulk("C8", "470u 25V", (108.5, 91), "6V", "SamYoung_BXJ_J10_C3002464")

# --- 6 V / 3.5 A servo buck ---

ap64352("U1", (78, 91), bst = "BST", rt = "RT", fb = "FB", ss = "SS", sw = "SW")
cya0850("L1", (88.5, 89.095), "SW", "6V")
for ref, at in (("C1", (68, 89.5)), ("C2", (71.5, 93.0)), ("C16", (68.0, 95.5))):
    two_pin(ref, "10u 50V", C0805, at, "VBAT", "GND", angle = 90)
two_pin("C3", "22u 25V", C1206, (96.5, 86.5), "6V", "GND")
two_pin("C4", "22u 25V", C1206, (96.5, 91.7), "6V", "GND")
two_pin("C5", "100n 50V", C0603, (78, 84.5), "BST", "SW")
two_pin("C9", "100n 50V", C0603, (84, 97), "SS", "GND")
two_pin("R1", "120k 1%", R0603, (92, 97), "6V", "FB_TOP", angle = 180)
two_pin("R2", "10k 1%", R0603, (89, 97), "FB_TOP", "FB", angle = 180)
two_pin("R3", "20k 1%", R0603, (86, 97), "FB", "GND", angle = 180)
two_pin("R4", "200k 1%", R0603, (75.5, 97), "RT", "GND", angle = 270)

track("SW", [pin("U1", "8"), (82.3, 89.095), pin("L1", "1")], width = 1.0)
track("VBAT", [pin("U1", "2"), (74.5, 90.365), (71.5, 90.45)], width = 1.0)
track("VBAT", [pin("U1", "3"), (73.5, 91.635), (73.5, 93.95), pin("C2", "1")], width = 1.0)
track("6V", [pin("L1", "2"), pin("TP4", "1"), (93.2, 86.5), pin("C3", "1")], width = 1.5)
track("6V", [pin("TP4", "1"), (93.2, 91.7), pin("C4", "1")], width = 1.5)
track("BST", [pin("U1", "1"), (75.5, 84.5), pin("C5", "1")], width = 0.35)
track("SW", [pin("C5", "2"), (80.5, 84.5), (80.5, 89.095)], width = 0.35)
track("RT", [pin("U1", "4"), pin("R4", "1")], width = 0.3)
track("SS", [pin("U1", "6"), (83.225, 91.635), pin("C9", "1")], width = 0.3)
track("FB", [pin("U1", "5"), (81.5, 92.905), (81.5, 99.2), (86.825, 99.2), pin("R3", "1")], width = 0.25)
track("FB", [pin("R3", "1"), pin("R2", "2")], width = 0.25)
track("FB_TOP", [pin("R2", "1"), pin("R1", "2")], width = 0.25)
track("6V", [pin("R1", "1"), (93.2, 97), (93.2, 91.7)], width = 0.25)
for points, width in [
    ([pin("U1", "9"), (78, 94)], 0.8),
    ([pin("U1", "7"), (82.3, 90.365)], 0.5),
    ([pin("C2", "2"), (69.8, 92.05)], 1.0),
    ([pin("C3", "2"), (99.3, 86.5)], 1.0),
    ([pin("C4", "2"), (99.3, 91.7)], 1.0),
    ([pin("C9", "2"), (84.775, 98.3)], 0.5),
    ([pin("R4", "2"), (75.5, 99)], 0.5),
]:
    # Short ground stubs from each return pad to its own via.
    track("GND", points, width = width)
    via("GND", points[-1], diameter = 0.7, drill = 0.3)
thermal_vias(pin("U1", "9"))

# --- 5 V control buck (same AP64352 part number as U1) ---

ap64352("U2", (44, 88), bst = "BST5", rt = "RT5", fb = "FB5", ss = "SS5", sw = "SW5")
cya0850("L2", (55.5, 86.095), "SW5", "5V")
for ref, y in (("C10", 85.5), ("C11", 89.2), ("C17", 92.5)):
    two_pin(ref, "10u 50V", C0805, (38, y), "VBAT", "GND", angle = 180)
track("VBAT", [pin("C17", "1"), pin("C11", "1")], width = 1.0)
track("GND", [pin("C17", "2"), pin("C11", "2")], width = 1.0)
two_pin("C12", "22u 25V", C1206, (63, 84.2), "5V", "GND")
two_pin("C13", "22u 25V", C1206, (63, 88.2), "5V", "GND")
two_pin("C14", "100n 50V", C0603, (44, 82.5), "BST5", "SW5")
two_pin("C15", "100n 50V", C0603, (48, 93.5), "GND", "SS5", angle = 90)
two_pin("R5", "200k 1%", R0603, (61, 93.5), "FB5_TOP", "5V")
two_pin("R6", "10k 1%", R0603, (58, 93.5), "FB5", "FB5_TOP")
two_pin("R7", "20k 1%", R0603, (55, 93.5), "FB5_BOTTOM", "FB5")
two_pin("R8", "20k 1%", R0603, (52, 93.5), "GND", "FB5_BOTTOM")
two_pin("R9", "200k 1%", R0603, (41, 93.5), "RT5", "GND", angle = 270)

track("SW5", [pin("U2", "8"), pin("L2", "1")], width = 1.0)
track("VBAT", [pin("U2", "2"), (39.7, 87.365), pin("C10", "1")], width = 1.0)
track("VBAT", [(39.7, 87.365), (39.7, 89.2), pin("C11", "1")], width = 1.0)
track("VBAT", [pin("U2", "3"), pin("U2", "2")], width = 0.35)
track("5V", [pin("L2", "2"), (60.6, 86.095), (60.6, 84.2), pin("C12", "1")], width = 1.0)
track("5V", [(60.6, 86.095), (60.6, 88.2), pin("C13", "1")], width = 1.0)
track("BST5", [pin("U2", "1"), (41.5, 82.5), pin("C14", "1")], width = 0.35)
track("SW5", [pin("C14", "2"), (46.5, 82.5), (46.5, 86.095)], width = 0.35)
track("RT5", [pin("U2", "4"), (41, 89.905), pin("R9", "1")], width = 0.3)
track("SS5", [pin("U2", "6"), (49, 88.635), (49, 92.725), pin("C15", "2")], width = 0.3)
track("FB5_BOTTOM", [pin("R8", "2"), pin("R7", "1")], width = 0.25)
track("FB5", [pin("R7", "2"), pin("R6", "1")], width = 0.25)
track("FB5_TOP", [pin("R6", "2"), pin("R5", "1")], width = 0.25)
track("5V", [pin("R5", "2"), (61.825, 92), (59, 92), (59, 88.2), pin("C13", "1")], width = 0.4)
for points, width in [
    ([pin("U2", "9"), (44, 91)], 0.8),
    ([pin("U2", "7"), (48.2, 87.365)], 0.5),
    ([pin("C10", "2"), (35.8, 85.5)], 0.8),
    ([pin("C11", "2"), (35.8, 89.2)], 0.8),
    ([(64.475, 84.2), (65.5, 84.2)], 0.8),
    ([(64.475, 88.2), (65.5, 88.2)], 0.8),
    ([pin("C15", "1"), (48, 95.2)], 0.4),
    ([pin("R8", "1"), (51.175, 94.8)], 0.4),
    ([pin("R9", "2"), (41, 95.2)], 0.4),
]:
    track("GND", points, width = width)
    via("GND", points[-1], diameter = 0.7, drill = 0.3)
# FB5 hops under the inductor on B.Cu.
track("FB5", [pin("U2", "5"), (47.5, 90.5)], width = 0.25)
via("FB5", (47.5, 90.5), diameter = 0.7, drill = 0.3)
track("FB5", [(47.5, 90.5), (56.0, 91.5)], width = 0.25, layer = "B.Cu")
via("FB5", (56.0, 91.5), diameter = 0.7, drill = 0.3)
track("FB5", [(56.0, 91.5), pin("R7", "2")], width = 0.25)
thermal_vias(pin("U2", "9"))

# --- Planes and fixed routes ---

# Back ground plane, notched under the ESP32 antenna.
zone("GND", "B.Cu", [(20.5, 20.5), (52, 20.5), (52, 37), (95, 37), (95, 20.5), (119.5, 20.5), (119.5, 119.5), (20.5, 119.5)])
zone("VBAT", "F.Cu", [(50, 75), (118, 75), (118, 105), (50, 105)], priority = 1)

# 5 V to the ESP32 socket on B.Cu, around the antenna keepout.
via("5V", (60.6, 86.095), diameter = 0.8, drill = 0.4)
track("5V", [(60.6, 86.095), (58, 82), (58, 40), pin("J7", "1")], width = 0.5, layer = "B.Cu")
track("VBAT", [pin("C10", "1"), (38.95, 80), (32, 80), (32, 100), (54.16, 100), pin("D1", "1")], width = 1.5)
track("VBAT", [pin("C7", "1"), (33.5, 78), (38.95, 80)], width = 1.5)
# Fixed Mini supply branches avoid the encoder and control pads; the wheel
# phases are left to the router and widened afterwards.
track("VBAT", [pin("J2", "2"), (33.8095, 62), (46, 65), (46, 72), (36, 72), pin("C7", "1")], width = 1.5)
track("VBAT", [pin("J3", "2"), (108, 64), (108, 80)], width = 1.5)
via("VBAT", pin("C1", "1"))
via("VBAT", (68, 80))
track("VBAT", [pin("C1", "1"), (68, 80)], width = 1.0, layer = "B.Cu")
# The right-servo signal splits the front VBAT pour; stitch both sides on B.Cu.
via("VBAT", (99, 80))
via("VBAT", (103, 80))
track("VBAT", [(99, 80), (103, 80)], width = 1.5, layer = "B.Cu")
# The control fan-out can divide the back ground pour near the right edge;
# this front-layer bridge keeps both regions on one return.
via("GND", (90, 39.54), diameter = 0.8, drill = 0.4)
track("GND", [pin("J8", "2"), (90, 39.54)], width = 0.5, layer = "B.Cu")
via("GND", (90, 72), diameter = 0.8, drill = 0.4)
track("GND", [(90, 39.54), (90, 72)], width = 1.0)
via("GND", (90, 80), diameter = 0.8, drill = 0.4)
track("GND", [(90, 72), (90, 80)], width = 0.6)
track("GND", [(90, 80), (90, 82), (102, 82), (102, 86.5), (99.3, 86.5)], width = 0.6, layer = "B.Cu")

# Right-wheel phases follow the corridor that routed successfully for M3.
track("M1_R", [pin("J15", "3"), (106.0, 54.572), (119.0, 54.572), (119.0, 78.0), pin("J17", "1")], width = 0.5, layer = "B.Cu")
track("M2_R", [pin("J15", "2"), (99.0, 48.0315), (106.0, 48.0315), (118.0, 60.0315), (118.0, 76.0), pin("J17", "2")], width = 0.5)
track("M3_R", [pin("J15", "1"), (99.0212, 50.5715), (105.7732, 50.5715), (116.0, 60.7983), pin("J17", "3")], width = 0.5)

# The right encoder's A/B/C leave J8 stacked and must arrive at J13 reversed,
# so they cross. F.Cu is walled off by the GND trunk at x=90 and IN3_R at
# x=91, so all three run east on B.Cu in stacked order, each on its own
# corridor, and drop to F.Cu only at their own pad column.
def hop(net, front_in, back, front_out = None, size = (0.6, 0.3)):
    """F.Cu → via → B.Cu → via → F.Cu, the fixed fan-out pattern used below.
    `front_in=None` starts on an existing F.Cu end; `front_out=None` ends on B.Cu."""
    if front_in:
        track(net, front_in, width = 0.2)
    via(net, back[0], diameter = size[0], drill = size[1])
    track(net, back, width = 0.2, layer = "B.Cu")
    if front_out:
        via(net, back[-1], diameter = size[0], drill = size[1])
        track(net, front_out, width = 0.2)

hop("A_R", [pin("J8", "6"), (87.5431, 50.8431), (88.9, 50.8431)],
    [(88.9, 50.8431), (89.8, 50.8431), (89.8, 57.5), (96.5, 57.5)], [(96.5, 57.5), pin("J13", "2")])
track("IN2_R", [pin("J8", "11"), (84.8034, 60.8034), (84.8034, 40.8), (93.3026, 40.8), (94.8235, 44.9489)], width = 0.2, layer = "B.Cu")
via("IN2_R", (94.8235, 44.9489), diameter = 0.6, drill = 0.3)

# Shared IN1_R, NFAULT_R, EN_R, and IN3_R branches are fixed; FreeRouting
# only completes the short corrected-header escapes.
track("IN1_R", [
    pin("J8", "10"), (85.1614, 58.6213), (83.84, 57.3), (83.84, 35.4157), (85.3715, 33.8842), (89.0746, 33.8842),
    (89.9763, 32.9825), (93.541, 32.9825), (94.4427, 33.8842), (95.7748, 33.8842), (100.9453, 39.0547),
], width = 0.2)
via("IN1_R", (100.9453, 39.0547), diameter = 0.6, drill = 0.3)
hop("IN1_R", [(85.1614, 58.6213), (83.4492, 60.3335)],
    [(83.4492, 60.3335), (83.4492, 62.8072), (80.997, 65.2594), (65.7612, 65.2594)],
    [(65.7612, 65.2594), (66.4345, 65.2595), pin("R19", "1")])

track("NFAULT_R", [pin("J8", "8"), (84.7417, 53.1217), (84.7417, 35.7892), (85.2616, 35.2693), (89.1569, 35.2693), (89.1569, 35.2692)], width = 0.2)
via("NFAULT_R", (89.1569, 35.2692), diameter = 0.6, drill = 0.3)
track("NFAULT_R", [
    (89.1569, 35.2692), (90.5419, 33.8842), (91.4629, 33.8842), (92.3286, 34.7499), (92.3286, 51.7088),
    # Stand off J13 pin 1: on the 2.5 mm XH pitch it sits at x=104.
    (106.2, 65.5802), (106.2, 68.3527), (103.0799, 71.4728), (100.9453, 71.4728),
], width = 0.2)
via("NFAULT_R", (100.9453, 71.4728), diameter = 0.6, drill = 0.3)
track("NFAULT_R", [pin("R13", "2"), (81.5748, 83.5918), (92.0082, 83.5918), pin("TP9", "1")], width = 0.2)
hop("NFAULT_R", [pin("TP9", "1"), (95.0511, 80.5489), (95.0511, 77.1654)],
    [(95.0511, 77.1654), (100.7437, 71.4728), (100.9453, 71.4728)])

track("EN_R", [pin("R22", "1"), (80.9453, 66.1203), (80.9454, 66.1203), (80.9454, 67.1054), pin("J8", "15")], width = 0.2)
track("EN_R", [
    pin("J8", "15"), (85.1111, 73.8489), (85.1111, 77.8268), (85.8216, 78.5373), (97.9427, 78.5373),
    (102.0601, 74.4199), (103.8924, 74.4199),
], width = 0.2, layer = "B.Cu")
via("EN_R", (103.8924, 74.4199), diameter = 0.6, drill = 0.3)
# Escape pin 9 outward before changing layers so it cannot cross pin 10.
track("EN_R", [pin("J12", "9"), (116.5, 41.9355)], width = 0.2)
via("EN_R", (116.5, 41.9355), diameter = 0.6, drill = 0.3)

via("IN3_R", (91.0, 35.0), diameter = 0.6, drill = 0.3)
hop("IN3_R", [(91.0, 35.0), (91.0, 69.0)], [(91.0, 69.0), (89.0, 69.0)], [(89.0, 69.0), pin("J8", "14")])
hop("IN3_R", [pin("J8", "14"), (82.6159, 66.2359), (82.6159, 66.2109)],
    [(82.6159, 66.2109), (76.5556, 66.2109), (74.5101, 68.2564)],
    [(74.5101, 68.2564), (75.175, 67.5915), pin("R21", "1")])

hop("SDA", [pin("J7", "6"), (55.0, 49.7)], [(55.0, 49.7), (55.0, 74.0)], [(55.0, 74.0), (60.0, 74.0)])
hop("SDA", None, [(60.0, 74.0), (72.9125, 74.0), (72.9125, 76.5)], [(72.9125, 76.5), pin("R11", "2")])

# 3V3 down the right edge, alternating layers around the phase routes.
def stitched(net, points, start_layer, width = 0.2, size = (0.6, 0.3)):
    """A straight run that changes layer (with a via) at every interior point."""
    layers = ["F.Cu", "B.Cu"] if start_layer == "F.Cu" else ["B.Cu", "F.Cu"]
    for i in range(len(points) - 1):
        if i > 0:
            via(net, points[i], diameter = size[0], drill = size[1])
        track(net, [points[i], points[i + 1]], width = width, layer = layers[i % 2])

track("3V3", [pin("J8", "1"), (95.0, 37.0), (104.0, 46.0), (104.0, 51.4)], width = 0.2, layer = "B.Cu")
via("3V3", (104.0, 51.4), diameter = 0.6, drill = 0.3)
stitched("3V3", [(104.0, 51.4), (104.0, 55.8), (104.0, 65.0), pin("J13", "5")], "F.Cu")

track("IN2_L", [pin("R16", "1"), (72.0, 60.825), (72.0, 54.78), pin("J7", "8")], width = 0.2)
# Fixed so FreeRouting cannot strand the ESP32 branch after joining the
# pull-down and servo header.
track("SERVO_L", [pin("J7", "3"), (80.0, 42.08), (80.0, 60.8), (67.0, 60.8), (65.175, 60.625)], width = 0.2, layer = "B.Cu")
via("SERVO_L", (65.175, 60.625), diameter = 0.6, drill = 0.3)
track("SERVO_L", [(65.175, 60.625), pin("R14", "1")], width = 0.2)
hop("IN3_L", [pin("R17", "1"), (75.0, 59.825)], [(75.0, 59.825), (75.0, 57.32), pin("J7", "9")])
hop("B_R", [pin("J8", "7"), (87.2998, 52.24), (88.85, 53.7902)],
    [(88.85, 53.7902), (88.85, 59.5), (99.0, 59.5)], [(99.0, 59.5), pin("J13", "3")])
# C_R starts lowest but owns the outermost pad, so it crosses A_R and B_R on
# F.Cu; the plated-through pad makes that free and keeps B.Cu crossing-free.
hop("C_R", [pin("J8", "9"), (88.7563, 59.6763), (88.7563, 60.1184)],
    [(88.7563, 60.1184), (88.7563, 62.5), (101.5, 62.5), pin("J13", "4")])
via("GND", (50, 39.54), diameter = 0.8, drill = 0.4)
track("GND", [pin("J7", "2"), (50, 39.54)], width = 0.5)
# Upper-right ground trunk, hopping layers around the M1_R route.
via("GND", (119.0, 53.0), diameter = 0.8, drill = 0.4)
track("GND", [(119.0, 53.0), (117.0, 56.5)], width = 0.6)
via("GND", (117.0, 56.5), diameter = 0.8, drill = 0.4)
track("GND", [(117.0, 56.5), pin("J3", "1"), (114, 64), (114, 70), (94.0, 70), pin("J13", "1")], width = 0.6, layer = "B.Cu")

# Right servo PWM: away from the edge, under the Mini supply branch on B.Cu,
# and a short B.Cu hop under C8's wide 6V branch.
track("SERVO_R", [pin("J8", "5"), (88, 47.16), (88, 74), (91, 74)], width = 0.4, layer = "B.Cu")
via("SERVO_R", (91, 74), diameter = 0.8, drill = 0.4)
track("SERVO_R", [(91, 74), (101, 74), (101, 104), (102.5, 104)], width = 0.4)
via("SERVO_R", (102.5, 104), diameter = 0.8, drill = 0.4)
track("SERVO_R", [(102.5, 104), (107.5, 104)], width = 0.4, layer = "B.Cu")
via("SERVO_R", (107.5, 104), diameter = 0.8, drill = 0.4)
track("SERVO_R", [(107.5, 104), (118, 104), (118, 113), pin("J5", "3")], width = 0.4)

# 6 V runs unbroken from L1 to the servo headers along the clear y=101
# corridor and the distribution trunk. (A 5 A fuse behind a 3.5 A regulator
# could never open, so there is none.)
track("6V", [(93.2, 91.7), (93.2, 101), (76.4, 101), (76.4, 107)], width = 2.0)
track("6V", [(76.4, 107), (113.54, 107), pin("J5", "2")], width = 2.0)
# FH1 carries the whole battery current: bond both pins of each terminal and
# leave on a 2 mm exit to the front pour. Columns: x 38.04 RAW, 47.96 VBAT.
track("VBAT_RAW", [(38.04, 109.3), (38.04, 112.7)], width = 2.0)  # both pins of terminal 1
track("VBAT", [(47.96, 109.3), (47.96, 112.7)], width = 2.0)  # both pins of terminal 2
track("VBAT", [pin("FH1", "2"), (49.4, 109.3), (49.4, 106), (52.0, 103.0)], width = 2.0)
track("6V", [(86.5, 107), pin("C6", "1")], width = 2.0)  # onto C6's positive pad
track("6V", [(105.54, 107), pin("J4", "2")], width = 2.0)
# C8 joins the bus on F.Cu; two local ground vias keep its pulse return short.
track("6V", [pin("C8", "1"), (105, 91), (105, 107)], width = 2.0)
track("GND", [pin("C8", "2"), (113, 93), (112, 93)], width = 1.5)
track("GND", [(113, 93), (114, 93)], width = 1.5)
via("GND", (112, 93), diameter = 0.8, drill = 0.4)
via("GND", (114, 93), diameter = 0.8, drill = 0.4)

for ref, at in (("H1", (24, 24)), ("H2", (116, 24)), ("H3", (24, 116)), ("H4", (116, 116))):
    mounting_hole(ref, at)

# --- Silkscreen ---

# The two look-alike bucks are boxed as whole functional blocks.
labelled_block((33.5, 80.4), (66.5, 100.4), "5V CONTROL REGULATOR", (50.0, 99.7))
labelled_block((66.8, 80.4), (100.7, 100.4), "6V SERVO REGULATOR", (83.75, 99.7))

text("BALANCE BOT", (70, 22.6), size = 1.45)
text("DAVID HOLTZ  /  drbh", (70, 25.2), size = 0.68)
text("ESP32 ANTENNA -> BOARD EDGE / KEEP CLEAR", (73.5, 30.2), size = 0.72)
for _, _, _, legend, at in TESTPOINTS:
    text(legend, at, size = 0.6)
# The fuse holder has no marking of its own: designator, rating, direction.
text("F1  10A MINI   VBAT_RAW -> VBAT", (43.0, 115.5), size = 0.6)

text("J1 BATTERY PIGTAIL", (32.2, 89.0), size = 0.50, angle = 90)
text("XT60-M PIGTAIL / 12.0-16.8 VDC", (35.4, 83.0), size = 0.50)
text("GND(-)  |  VBAT_RAW(+)", (35.4, 84.3), size = 0.50)
# Per-pad polarity right at the pads: a 4S pack, no reverse protection, and
# no fuse until F1 is fitted make this the one connector that must not be guessed.
pin_legends("J1", {"1": "-", "2": "+"}, column = 30.0, size = 1.1)

for x, net in ((33.81, "VBAT"), (38.76, "GND"), (105.81, "VBAT"), (110.76, "GND")):
    text(net, (x, 62.0), size = 0.60)
pin_legends("J4", {"1": "GND", "2": "6V", "3": "PWM-L"}, row = 110)
pin_legends("J5", {"1": "GND", "2": "6V", "3": "PWM-R"}, row = 110)
for name, y in (("M1", 45.49), ("M2", 48.03), ("M3", 50.57)):
    text(name, (28.0, y), size = 0.60)
    text(name, (100.0, y), size = 0.60)

text("J16 MOTOR L", (25.6, 70.8), size = 0.52)
text("J17 MOTOR R", (114.4, 70.8), size = 0.52)
pin_legends("J16", {"1": "U", "2": "V", "3": "W"}, column = 28.0, size = 0.62)
pin_legends("J17", {"1": "U", "2": "V", "3": "W"}, column = 112.0, size = 0.62)

# C7 hides the area below J10, so its callout sits beside the connector.
ENCODER_LEGENDS = {"1": "GND", "2": "A", "3": "B", "4": "C", "5": "3V3"}
text("<- J10 ENC L", (47.0, 67.4), size = 0.52)
pin_legends("J10", ENCODER_LEGENDS, row = 63.8)
text("J13 ENC R", (100.0, 70.8), size = 0.52)
pin_legends("J13", ENCODER_LEGENDS, row = 63.8)
pin_legends("J11", {"1": "GND", "2": "SCL", "3": "SDA", "4": "3V3"}, row = 80.5)

text("J7 ESP32 LEFT ROW", (54.5, 35), size = 0.65)
text("PIN 1 = 5V", (56.5, 38), size = 0.6, angle = 90)
text("J8 ESP32 RIGHT ROW", (93, 35), size = 0.65)
text("PIN 1 = 3V3", (91, 38), size = 0.6, angle = 90)
text("BALANCE BOT  /  DAVID HOLTZ  /  drbh", (70, 118), size = 0.72)

# Cat portraits: small on the front corners beside the title, large on the
# otherwise text-free back.
for name, image, front_x, back_x in (("BIGGIE", "biggie.png", 35.0, 45.0), ("JAMEO", "jameo.png", 105.0, 95.0)):
    for side, at, width, layer in (("FRONT", (front_x, 29.5), 12.5, "F.SilkS"), ("BACK", (back_x, 70.0), 43.0, "B.SilkS")):
        ref = "LOGO_%s_%s" % (name, side)
        artwork(ref, image, center = at, width = width, layer = layer, value = image + " / WHITE SILK",
                footprint = "BalanceBot_Devboard:Cat_Silkscreen_" + ref)

# --- After autorouting ---

# Routed tracks get their net's minimum width, except the VBAT input leads
# inside the two regulator blocks, which stay 1.0 mm to clear the pads.
MIN_WIDTH = {"VBAT_RAW": 1.0, "VBAT": 1.5, "M1_L": 0.5, "M2_L": 0.5, "M3_L": 0.5, "M1_R": 0.5, "M2_R": 0.5, "M3_R": 0.5}

def route_width(t):
    if t.net == "VBAT":
        xs, ys = (t.start[0], t.end[0]), (t.start[1], t.end[1])
        near_6v = max(xs) > 70 and min(ys) > 84 and max(ys) < 96
        near_5v = min(xs) > 35 and max(xs) < 45 and min(ys) > 83 and max(ys) < 92
        if near_6v or near_5v:
            return 1.0
    wanted = MIN_WIDTH.get(t.net)
    if wanted and t.width < wanted:
        return wanted
    return None

# Fixed segments the router may thin or drop.
keep_track("6V", pin("TP4", "1"), (93.2, 86.5), width = 1.5)
keep_track("BST", pin("U1", "1"), (75.5, 84.5), width = 0.35)
keep_track("SW5", pin("C14", "2"), (46.5, 82.5), width = 0.35)
keep_track("VBAT", (71.5, 90.45), pin("C1", "1"), width = 1.0)
