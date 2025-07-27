# Linux Shell Control Commands

Turning the display on or off is possible directly in a Linux shell!

Add your user to the `dialout` group for access to `/dev/ttyACM0`:

```shell
sudo usermod -a -G dialout $USER
```

> You may have to log out and back in for group changes to take effect.  
> If not using a Debian based Linux, the tty device might have a different name, or not using the `dialout` group.


## Turn display on

```shell
stty -F /dev/ttyACM0 raw
printf "\252U\252U\v\0\0\0" > /dev/ttyACM0
```

## Turn display off

```shell
stty -F /dev/ttyACM0 raw
printf "\252U\252U\12\0\0\0" > /dev/ttyACM0
```
