# API Reference: test_doc_generator.ep

## Structures

### `structure Point`

This is a structure representing a Point in 2D space.

**Fields:**
- `x` as `Int`
- `y` as `Int`

## Choices

### `choice Color`

This is a choice representing a Color variant.

**Variants:**
- `variant Red`
- `variant Green` with `intensity` as `Int`

## Traits

### `trait Area`

This is a trait for calculating area.

**Methods:**
- `define get_area returning Int`

## Functions

### `define get_distance with a as Point and b as Point returning Int`

Calculate distance between two coordinates.

## Methods

### `define draw_point on Point with color as Color returning Int`

Draw a point with a color.

