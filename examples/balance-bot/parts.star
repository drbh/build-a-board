"""Reusable components for the Balance Bot devboard.

Every function places parts with bb's primitives; none of them know where
on this particular board they end up.
"""

LIB = "BalanceBot_Devboard"
MODELS = "${KICAD10_3DMODEL_DIR}"
R0603 = "Resistor_SMD:R_0603_1608Metric"
C0603 = "Capacitor_SMD:C_0603_1608Metric"
C0805 = "Capacitor_SMD:C_0805_2012Metric"
C1206 = "Capacitor_SMD:C_1206_3216Metric"

def numbered(nets):
    """Pin map for a connector whose pads are numbered 1..n in `nets` order."""
    return {str(i + 1): net for i, net in enumerate(nets)}

def two_pin(ref, value, footprint, at, a, b, angle = 0):
    """A resistor or capacitor between nets `a` (pad 1) and `b` (pad 2)."""
    part(ref, value, footprint, at = at, angle = angle, pins = {"1": a, "2": b})

def testpoint(ref, net, at):
    part(ref, net, "TestPoint:TestPoint_Pad_D1.5mm", at = at, pins = {"1": net})

def fiducial(ref, at):
    part(ref, "FIDUCIAL", "Fiducial:Fiducial_1mm_Mask2mm", at = at)

def mounting_hole(ref, at):
    part(ref, "M3", "MountingHole:MountingHole_3.2mm_M3", at = at)

def ap64352(ref, at, bst, rt, fb, ss, sw):
    """Diodes AP64352 40 V / 3.5 A synchronous buck in SO-8EP.

    KiCad 10 ships the exact footprint without its named model; the standard
    3.9 x 4.9 mm SO-8 body is mechanically representative.
    """
    part(
        ref,
        "AP64352SP-13",
        "Package_SO:Diodes_SO-8EP",
        at = at,
        pins = {"1": bst, "2": "VBAT", "3": "VBAT", "4": rt, "5": fb, "6": ss, "7": "GND", "8": sw, "9": "GND"},
        swap_model = MODELS + "/Package_SO.3dshapes/SO-8_3.9x4.9mm_P1.27mm.step",
    )

def thermal_vias(center, reach = 2.25):
    """Six small vias above and below an exposed pad, each tied back to the
    pad centre by a short spoke into the B.Cu ground plane."""
    cx, cy = center
    for x in (cx - 0.65, cx, cx + 0.65):
        for y in (cy - reach, cy + reach):
            track("GND", [center, (x, y)])
            via("GND", (x, y), diameter = 0.6, drill = 0.3)

def cya0850(ref, at, input, output):
    """SHOU HAN CYA0850: 9.2 x 8 mm body on JLC/LCSC's 2.5 x 3.8 mm pads."""
    generated(
        ref,
        "10uH 5A / C19268645",
        LIB + ":CYA0850_10uH",
        at = at,
        pads = [smd(1, (-3.7, 0), (2.5, 3.8), net = input), smd(2, (3.7, 0), (2.5, 3.8), net = output)],
        body = (9.2, 8.0),
        models = [model(MODELS + "/Inductor_SMD.3dshapes/L_Coilcraft_XAL8050-223.step")],
    )

def vchip_bulk(ref, value, at, positive, name):
    """10 mm SMD electrolytic on JLC's exact V-chip land pattern.

    Pad 1 is the positive terminal, matching JLC's own footprints. The
    polarity silk mirrors them too: chamfered corners on the positive side,
    '+' beside pad 1 and '-' beside pad 2, checkable before and after reflow.
    """
    outline = [
        ((5.23, -0.85), (5.23, -5.23)),
        ((5.23, -5.23), (-3.09, -5.23)),
        ((-3.09, -5.23), (-5.23, -3.09)),
        ((-5.23, -3.09), (-5.23, -0.85)),
        ((5.23, 0.85), (5.23, 5.23)),
        ((5.23, 5.23), (-3.09, 5.23)),
        ((-3.09, 5.23), (-5.23, 3.09)),
        ((-5.23, 3.09), (-5.23, 0.85)),
    ]
    marks = [((-4.84, 2.37), (-3.61, 2.37)), ((-4.22, 1.34), (-4.22, 3.40)), ((3.60, 2.37), (4.84, 2.37))]
    generated(
        ref,
        value,
        LIB + ":" + name,
        at = at,
        pads = [smd(1, (-4.5, 0), (4.5, 1.65), net = positive), smd(2, (4.5, 0), (4.5, 1.65), net = "GND")],
        body = (10.0, 10.0),
        graphics = [fp_line("F.SilkS", a, b, width = 0.15) for a, b in outline] +
                   [fp_line("F.SilkS", a, b, width = 0.31) for a, b in marks],
        models = [model(MODELS + "/Capacitor_SMD.3dshapes/CP_Elec_10x10.step")],
    )

def fuse_holder(ref, value, center, left, right):
    """Keystone 3568 holder for one MINI/ATM blade fuse.

    Each terminal lands two 1.78 mm pins 3.4 mm apart and the terminals sit on
    a 9.92 mm pitch, so `center` is the middle of the four-hole pattern, not
    the footprint origin. MINI rather than ATO: both interrupt 1 kA at 32 V,
    which is what matters against a 4S pack, but the MINI holder is a quarter
    of the price, has four holes instead of nine, and has a curated KiCad land
    pattern. The original hand-drawn clip footprint was wrong by the width of
    the fuse body.
    """
    x, y = center
    part(ref, value, "Fuse:Fuseholder_Blade_Mini_Keystone_3568", at = (x - 4.96, y - 1.7), pins = {"1": left, "2": right})

def simplefoc_mini_v1(side, center, angle, control, power, phase):
    """Direct-solder carrier for an official SimpleFOC Mini v1.0.

    Geometry is transcribed from the tagged v1.0 EasyEDA source. A 3 A male
    breakaway strip is cut into control, phase, and individual power sections;
    the short posts solder into this carrier and the long pins pass through
    the Mini, soldered permanently after alignment.
    """
    suffix = "L" if side == "LEFT" else "R"
    enable = "EN" if side == "LEFT" else "EN_R"
    header_angle = (angle + 90) % 360

    def world(local):
        return rotate(center, local, angle)

    # Header locations are relative to the module's mechanical centre
    # (outline ~23.0 x 20.955 mm), with numbering intact. Use the male-header
    # footprint itself: KiCad's 2xN socket puts its second column on the other
    # side of pad 1, which would put one hole column outside the Mini.
    cx, cy = (-2.9845, -7.3035)
    part(
        control,
        "SIMPLEFOC MINI V1.0 %s DIRECT CONTROL PINS" % side,
        "Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical",
        at = world((cx + 5.08, cy - 1.27)),
        angle = header_angle,
        pins = {
            "1": "GND",
            "3": "IN1_" + suffix,
            "4": "GND",
            "5": "IN2_" + suffix,
            "7": "IN3_" + suffix,
            "9": enable,
            "10": "NFAULT_" + suffix,
        },
    )

    # Phase header: official pin 1=M3, 2=M2, 3=M1.
    px, py = (-1.9685, 9.0805)
    part(
        phase,
        "SIMPLEFOC MINI V1.0 %s DIRECT PHASE PINS" % side,
        "Connector_PinSocket_2.54mm:PinSocket_1x03_P2.54mm_Vertical",
        at = world((px + 2.54, py)),
        angle = header_angle,
        pins = {"1": "M3_" + suffix, "2": "M2_" + suffix, "3": "M1_" + suffix},
        models = [model(MODELS + "/Connector_PinHeader_2.54mm.3dshapes/PinHeader_1x03_P2.54mm_Vertical.step")],
    )

    # Two separately cut 1x1 pins at the Mini's exact power-hole centres: the
    # holes sit on a non-standard 4.953 mm pitch.
    gnd = world((9.3345, -4.7625))
    vbat = world((9.3345, 0.1905))
    mid = ((gnd[0] + vbat[0]) / 2, (gnd[1] + vbat[1]) / 2)
    offsets = [(p[0] - mid[0], p[1] - mid[1]) for p in (gnd, vbat)]
    pin = MODELS + "/Connector_PinHeader_2.54mm.3dshapes/PinHeader_1x01_P2.54mm_Vertical.step"
    generated(
        power,
        "SIMPLEFOC MINI V1.0 %s DIRECT POWER PINS" % side,
        LIB + ":Generated_" + power,
        at = mid,
        pads = [
            tht(1, offsets[0], (2.0, 2.0), drill = 1.0, net = "GND"),
            tht(2, offsets[1], (2.0, 2.0), drill = 1.0, net = "VBAT", shape = "circle"),
        ],
        body = (6.5, 3.0) if angle in (0, 180) else (3.0, 6.5),
        models = [model(pin, offset = (x, y, 0)) for x, y in offsets],
    )

    # The permanently mounted module's envelope and orientation, with the same
    # caption on both modules so their rotated outlines read identically.
    corners = [(-11.5, -10.4775), (11.5, -10.4775), (11.5, 10.4775), (-11.5, 10.4775)]
    outline("F.SilkS", [world(c) for c in corners], width = 0.25)
    text("SIMPLEFOC MINI v1.0 / SOLDER", center, size = 0.62, angle = angle)

def labelled_block(top_left, bottom_right, caption, caption_at):
    """A durable silkscreen box around a functional block, captioned below."""
    (x1, y1), (x2, y2) = top_left, bottom_right
    outline("F.SilkS", [(x1, y1), (x2, y1), (x2, y2), (x1, y2)], width = 0.3)
    text(caption, caption_at, size = 0.64)
