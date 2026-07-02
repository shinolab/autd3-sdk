from autd3.commands import GpioOut, SetGpioOut

gpio0 = GpioOut.PatternBank
gpio1 = GpioOut.Thermo
gpio2 = GpioOut.PwmOut(0)
gpio3 = GpioOut.Off
outputs = [gpio0, gpio1, gpio2, gpio3]
# ANCHOR: api
SetGpioOut(outputs=outputs)
# ANCHOR_END: api
