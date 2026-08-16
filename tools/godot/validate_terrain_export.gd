extends SceneTree

const TILESET_PATH := "res://exports/godot/lesser-antilles-volcanic-quay-tileset-v1-tileset/lesser-antilles-volcanic-quay-tileset-v1-tileset.tres"
const BITS := {
	TileSet.CELL_NEIGHBOR_TOP_SIDE: 1,
	TileSet.CELL_NEIGHBOR_TOP_RIGHT_CORNER: 2,
	TileSet.CELL_NEIGHBOR_RIGHT_SIDE: 4,
	TileSet.CELL_NEIGHBOR_BOTTOM_RIGHT_CORNER: 8,
	TileSet.CELL_NEIGHBOR_BOTTOM_SIDE: 16,
	TileSet.CELL_NEIGHBOR_BOTTOM_LEFT_CORNER: 32,
	TileSet.CELL_NEIGHBOR_LEFT_SIDE: 64,
	TileSet.CELL_NEIGHBOR_TOP_LEFT_CORNER: 128,
}
const OFFSETS := {
	1: Vector2i(0, -1), 2: Vector2i(1, -1), 4: Vector2i(1, 0),
	8: Vector2i(1, 1), 16: Vector2i(0, 1), 32: Vector2i(-1, 1),
	64: Vector2i(-1, 0), 128: Vector2i(-1, -1),
}


func _initialize() -> void:
	var tileset := load(TILESET_PATH) as TileSet
	if tileset == null:
		return fail("Could not load the Sprite Studio TileSet", 1)
	var atlas := tileset.get_source(0) as TileSetAtlasSource
	if atlas == null or atlas.get_tiles_count() != 47:
		return fail("Expected exactly 47 generated atlas tiles", 2)
	if atlas.texture_region_size != Vector2i(16, 16):
		return fail("Expected 16x16 texture regions", 3)
	if tileset.get_terrain_set_mode(0) != TileSet.TERRAIN_MODE_MATCH_CORNERS_AND_SIDES:
		return fail("Expected corners-and-sides terrain matching", 4)

	var masks := {}
	for index in atlas.get_tiles_count():
		var coordinates := atlas.get_tile_id(index)
		var data := atlas.get_tile_data(coordinates, 0)
		if data.terrain_set != 0 or data.terrain != 0:
			return fail("Atlas tile %s is missing its terrain assignment" % coordinates, 5)
		var mask := tile_mask(data)
		if masks.has(mask):
			return fail("Duplicate terrain mask %d" % mask, 6)
		masks[mask] = coordinates
	if masks.size() != 47 or not masks.has(0) or not masks.has(255):
		return fail("The atlas does not contain the canonical 47-mask set", 7)

	var cases := {
		"concave_corners": concave_cells(),
		"islands": island_cells(),
		"one_cell_channels": channel_cells(),
	}
	for case_name in cases:
		var error := validate_shape(tileset, cases[case_name])
		if not error.is_empty():
			return fail("%s: %s" % [case_name, error], 8)
	print("SPRITE_STUDIO_TERRAIN_47_OK tiles=47 masks=47 concave=pass islands=pass channels=pass")
	quit()


func tile_mask(data: TileData) -> int:
	var mask := 0
	for neighbor in BITS:
		if data.get_terrain_peering_bit(neighbor) == 0:
			mask |= BITS[neighbor]
	return mask


func expected_mask(cell: Vector2i, occupied: Dictionary) -> int:
	var mask := 0
	for bit in OFFSETS:
		if occupied.has(cell + OFFSETS[bit]):
			mask |= bit
	# Corners only participate when both adjoining sides are occupied.
	if mask & 1 == 0 or mask & 4 == 0: mask &= ~2
	if mask & 4 == 0 or mask & 16 == 0: mask &= ~8
	if mask & 16 == 0 or mask & 64 == 0: mask &= ~32
	if mask & 64 == 0 or mask & 1 == 0: mask &= ~128
	return mask


func validate_shape(tileset: TileSet, cells: Array[Vector2i]) -> String:
	var occupied := {}
	for cell in cells: occupied[cell] = true
	var layer := TileMapLayer.new()
	layer.tile_set = tileset
	layer.set_cells_terrain_connect(cells, 0, 0, false)
	var atlas := tileset.get_source(0) as TileSetAtlasSource
	for cell in cells:
		if layer.get_cell_source_id(cell) != 0:
			layer.free()
			return "cell %s was not placed" % cell
		var data := atlas.get_tile_data(layer.get_cell_atlas_coords(cell), layer.get_cell_alternative_tile(cell))
		var expected := expected_mask(cell, occupied)
		var actual := tile_mask(data)
		if actual != expected:
			layer.free()
			return "cell %s expected mask %d but Godot selected %d" % [cell, expected, actual]
	layer.free()
	return ""


func concave_cells() -> Array[Vector2i]:
	var cells: Array[Vector2i] = []
	for y in 5:
		for x in 6:
			if not (x >= 3 and y <= 1) and not (x <= 1 and y >= 4): cells.append(Vector2i(x, y))
	return cells


func island_cells() -> Array[Vector2i]:
	return [Vector2i(0, 0), Vector2i(3, 0), Vector2i(6, 0), Vector2i(1, 3), Vector2i(4, 3), Vector2i(7, 3)]


func channel_cells() -> Array[Vector2i]:
	return [
		Vector2i(0, 2), Vector2i(1, 2), Vector2i(2, 2), Vector2i(3, 2),
		Vector2i(4, 2), Vector2i(5, 2), Vector2i(6, 2), Vector2i(3, 0),
		Vector2i(3, 1), Vector2i(3, 3), Vector2i(3, 4), Vector2i(3, 5),
	]


func fail(message: String, code: int) -> void:
	push_error(message)
	quit(code)
