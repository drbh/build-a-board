load("lib.star", "resistor")

project("fixture", lcsc = {"MPN": "C1"})
board(hide_references = ["J"])
nets(["VIN", "GND"])
resistor("R1", (10, 10), "VIN", "GND")
resistor("R2", (20, 10), "VIN", "GND", angle = 270)
track("GND", [pin("R1", 2), (15, 12), pin("R2", "2")], width = 0.3)
via("GND", (15, 12), diameter = 0.6, drill = 0.3)
pin_legends("R2", {"1": "+", "2": "-"}, column = 22)

def route_width(t):
    return 0.5 if t.net == "GND" else None
