# Custom Panels

By default, the defined panels in the main configuration file are loaded and rendered.

Additional custom panels can be included with the `--panels` command line parameter.

A custom panel consists of:
- a `panel.json` file with just the json object of the `diy` array of the main configuration file.
- `img` subdirectory containing the referenced images in `panel.json`
- `fonts` subdirectory containing the referenced fonts in `panel.json`

Example:
```
.
├── fonts
│   ├── HarmonyOS_Sans_SC_Bold.ttf
│   └── ROGFontsv.ttf
├── img
│   ├── 6ae90fde-d0a1-44ec-9e15-7b6af14e3b7b.jpg
│   ├── 95f38f70-9e0c-4b54-80a9-6bd7b0b4475c_1744449208_1746941971.png
│   ├── f1c3d74c-0157-4b77-82a6-f07e565fe439_1744447224_1746941971.png
│   └── f5d534e5-4527-4ca0-a0e9-69e8eef86f62_1744447151_1746941971.png
└── panel.json
```

There are lots of custom panel configurations available online.
AOOSTAR support sent the following link: <http://pan.sztbkj.com:5244/>, look for a file called [`有线网卡 windows驱动.rar`](http://pan.sztbkj.com:5244/WTR%20MAX%206+5%E7%9B%98%E4%BD%8D/%E9%A9%B1%E5%8A%A8/%E6%9C%89%E7%BA%BF%E7%BD%91%E5%8D%A1%20windows%E9%A9%B1%E5%8A%A8.rar)
in the `WTR MAX 6+5盘位/驱动/` subfolder.

Example:
```shell
asterctl --config monitor.json --panels cfg/01_custom --panels cfg/02_custom
```
