# Sensor Mode 3 Progress

A progress sensor, known as `fan` sensor in AOOSTAR-X, masks a circular progress image based on the corresponding sensor value and overlays it on the panel.

Sensor configuration fields:
- `mode`: 3 (for progress)
- `label`: label identifier, also used as sensor value data source identifier
- `direction`: 1 = left to right, 2 = right to left, 3 = top to bottom, 4 = bottom to top
- `x`, `y`: position on the panel
- `pic`: progress image to crop and overlay
- `minValue`, `maxValue`: clamp sensor value to this range

## Example

Example `panel.json` with a single "progress" indicator sensor:

```json
{
  "name": "Progress test panel",
  "img": "background.jpg",
  "sensor": [
    {
      "mode": 3,
      "type": 2,
      "name": "HD usage",
      "label": "storage_usage",
      "x": 50,
      "y": 220,
      "direction": 1,
      "value": "15",
      "fontFamily": "",
      "fontSize": 14,
      "fontWeight": "normal",
      "textAlign": "center",
      "minValue": 0,
      "maxValue": 100,
      "pic": "progress.png"
    }
  ]
}
```

Pointer image `"pic": "progress.png"`:

![progress graphic](img/progress.png)

TODO rendered sensor image

## Known Issues

- Work in progress, not yet fully tested
- `widht`, `height` should be considered and auto-resized as for mode 4
