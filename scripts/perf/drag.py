import sys, time
from Xlib import X, display
from Xlib.ext import xtest
d = display.Display(":99")
x0, y0 = int(sys.argv[1]), int(sys.argv[2])
def mv(x, y): xtest.fake_input(d, X.MotionNotify, x=int(x), y=int(y)); d.sync()
mv(x0, y0); time.sleep(0.5); mv(x0+1, y0); time.sleep(0.4); mv(x0, y0); time.sleep(0.4)
# A first click brings the window to the front: without it, the following drag
# is sometimes lost (the window is not yet the layer under the pointer).
xtest.fake_input(d, X.ButtonPress, 1); d.sync(); time.sleep(0.1)
xtest.fake_input(d, X.ButtonRelease, 1); d.sync(); time.sleep(1.0)
xtest.fake_input(d, X.ButtonPress, 1); d.sync(); time.sleep(0.3)
import math
for i in range(1, 301):
    mv(x0 + 60*math.sin(i/15) + 4, y0 + 20*math.sin(i/20) + 1); time.sleep(0.02)
time.sleep(0.3)
xtest.fake_input(d, X.ButtonRelease, 1); d.sync(); time.sleep(0.5)
