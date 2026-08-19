extends Node2D

const CELL_SIZE := 64
const GRID_ORIGIN := Vector2(64, 86)
const CASE_NAMES := ["Concave corners", "Islands", "One-cell channels"]

@onready var terrain: TileMapLayer = $Terrain
@onready var mode_label: Label = $Mode

var case_index := 0


func _ready() -> void:
	populate_grid()
	queue_redraw()


func populate_grid() -> void:
	terrain.clear()
	var cells := case_cells(case_index)
	terrain.set_cells_terrain_connect(cells, 0, 0, false)
	var placed := 0
	for cell in cells:
		if terrain.get_cell_source_id(cell) == 0: placed += 1
	mode_label.text = "%s · %d/%d matched · Space to cycle" % [CASE_NAMES[case_index], placed, cells.size()]


func _unhandled_key_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		get_tree().quit()
	elif event.is_action_pressed("ui_accept"):
		case_index = (case_index + 1) % CASE_NAMES.size()
		populate_grid()


func case_cells(index: int) -> Array[Vector2i]:
	if index == 1:
		return [Vector2i(0, 0), Vector2i(3, 0), Vector2i(6, 0), Vector2i(1, 3), Vector2i(4, 3), Vector2i(7, 3)]
	if index == 2:
		return [
			Vector2i(0, 2), Vector2i(1, 2), Vector2i(2, 2), Vector2i(3, 2),
			Vector2i(4, 2), Vector2i(5, 2), Vector2i(6, 2), Vector2i(3, 0),
			Vector2i(3, 1), Vector2i(3, 3), Vector2i(3, 4), Vector2i(3, 5),
		]
	var cells: Array[Vector2i] = []
	for y in 5:
		for x in 6:
			if not (x >= 3 and y <= 1) and not (x <= 1 and y >= 4): cells.append(Vector2i(x, y))
	return cells


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, Vector2(640, 540)), Color("101715"))
	draw_rect(Rect2(GRID_ORIGIN - Vector2(5, 5), Vector2(8 * CELL_SIZE, 6 * CELL_SIZE) + Vector2(10, 10)), Color("26342f"), true)
	for x in range(9):
		var px := GRID_ORIGIN.x + x * CELL_SIZE
		draw_line(Vector2(px, GRID_ORIGIN.y), Vector2(px, GRID_ORIGIN.y + 6 * CELL_SIZE), Color(1, 1, 1, 0.08), 1.0)
	for y in range(7):
		var py := GRID_ORIGIN.y + y * CELL_SIZE
		draw_line(Vector2(GRID_ORIGIN.x, py), Vector2(GRID_ORIGIN.x + 8 * CELL_SIZE, py), Color(1, 1, 1, 0.08), 1.0)
