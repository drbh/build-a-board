def resistor(ref, at, a, b, angle = 0):
    part(ref, "10k", "Test:R_Test", at = at, angle = angle, pins = {"1": a, "2": b})
