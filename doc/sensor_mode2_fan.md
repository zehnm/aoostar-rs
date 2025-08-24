# Sensor Mode 2 Circular Progress

A circular progress sensor crops a progress bar image at a certain position based on the corresponding sensor value and overlays it on the panel.

Sensor configuration fields:
- `mode`: 2 (for fan)

## Example

Example `panel.json` with a single "fan" indicator sensor:

```json
{
  "name": "Fan test panel",
  "img": "background.jpg",
  "sensor": [
  ]
}

```

## Known Issues

- Work in progress, not yet fully tested
