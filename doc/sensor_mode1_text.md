# Sensor Mode 1 Text

A text sensor renders a text label with a sensor value and an optional unit text on the panel. 
The value can be formatted as a fixed point decimal number or as an integer.

Text sensor configuration fields:
- `mode`: 1 (for text)
- `label`: label identifier, also used as sensor value data source identifier
- `direction`: 1 = left to right, 2 = right to left, 3 = top to bottom, 4 = bottom to top
- `label`: data source id to retrieve the current value from
- `unit`: optional unit label, appended after the sensor value 
- `x`, `y`: position on the panel
- `fontFamily`: Font name matching font filename without file extension.
  - Fonts are loaded from the configured font directory, or from the custom panel's `fonts` directory. 
  - An absolute file path can also be used.
- `fontSize`: Font size
- `fontColor`: Font color in `#RRGGBB` notation, or `-1` if not set.
  - Examples: `#ffffff` = white, `#ff0000` = red. Default: `#ffffff`
- `textAlign`: Text alignment: `left`, `center`, `right`
- `integerDigits`:
- `decimalDigits`:

## Example

Example `panel.json` with a single "text" indicator sensor:

```json
{
  "name": "Text test panel",
  "img": "background.jpg",
  "sensor": [
    {
      "mode": 1,
      "type": 1,
      "name": "CPU usage",
      "label": "cpu_percent",
      "unit": "%",
      "x": 200,
      "y": 285,
      "value": "98",
      "fontFamily": "HarmonyOS_Sans_SC_Bold",
      "fontSize": 60,
      "fontColor": -1,
      "fontWeight": "normal",
      "textAlign": "center",
      "integerDigits": -1,
      "decimalDigits": 0
    }
  ]
}
```

TODO rendered sensor image

## Known Issues

- Text position doesn't always match AOOSTAR-X
- Font size calculation might not be accurate. Needs investigation if value is pixel or points.
- `fontWeight` not yet supported
