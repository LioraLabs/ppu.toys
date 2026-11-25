extends Node2D
## Marks the units a check flagged, in canvas pixel space; touches no layer or selection.

var rects: Array[Rect2] = []
var outline := Rect2()  # a finding without a place (a PLTE problem) outlines the image


func _draw() -> void:
	for r in rects:
		draw_rect(r, Color(1, 0.1, 0.1, 0.45), true)
		draw_rect(r.grow(0.5), Color(1, 0.9, 0), false, 1.0)
	if outline.size != Vector2.ZERO:
		draw_rect(outline.grow(1), Color(1, 0.5, 0), false, 3.0)
