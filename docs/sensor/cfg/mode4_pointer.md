# Sensor Mode 4 Pointer

A pointer sensor rotates an image at a certain angle calculated from the current sensor value and alpha-blends it with
the panel image.

Sensor configuration fields:
- `mode`: 4 (for pointer)
- `direction`: 1 = clockwise, 2 = counter-clockwise
- `label`: label identifier, also used as sensor value data source identifier
- `x`, `y`: position on the panel
- `width`, `height`: size of the pointer
- `pic`: pointer image to overlay. Should match `width`, `height`, otherwise it will be resized
- `minAngle`, `maxAngle`: range of the rotated image
- `minValue`, `maxValue`: scaling range to apply on the value for `minAngle` .. `maxAngle`  (to be verified)
- `xz_x`, `xz_y`

## Example

The following configuration and graphics are taken from the `三环_windows` panel configuration in `有线网卡 windows驱动.rar`.

Example `panel.json` with a single "pointer" indicator sensor and the following (partial) background image in `img`:

<img src="../../img/sensor_mode4_background.png" alt="sensor mode 4 background image example">

```json
{
  "name": "Pointer test panel",
  "img": "background.jpg",
  "sensor": [
    {
      "id": "a9d4acac-2af9-4fe0-9f69-86cd09f25696",
      "itemName": "CPU dial",
      "mode": 4,
      "direction": 1,
      "label": "cpu_percent",
      "value": "47.7",
      "x": 160,
      "y": 208,
      "width": 302,
      "height": 302,
      "minAngle": -110,
      "maxAngle": 110,
      "minValue": 0,
      "maxValue": 90,
      "xz_x": 0,
      "xz_y": 0,
      "pic": "pointer.png"
    }
  ]
}
```

Pointer image `"pic": "pointer.png"`:

![pointer graphic](../../img/mode4_pic.png)

The following graphic is rendered for a sensor value of `47.7`:

<img src="../../img/sensor_mode4.png" alt="sensor mode 4 example">

## Known Issues

Pointer sensor rendering has been reverse engineered from the AOOSTAR-X app. Not all options are supported.

- Work in progress, not yet fully tested
