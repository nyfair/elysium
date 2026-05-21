import argparse
import ctypes
from ctypes import wintypes
import time
from PIL import Image
import numpy
import vgamepad

TITLE = "异环  "
WIDTH = 1280
HEIGHT = 720

class RECT(ctypes.Structure):
  _fields_ = [
    ("left",   wintypes.LONG),
    ("top",    wintypes.LONG),
    ("right",  wintypes.LONG),
    ("bottom", wintypes.LONG)
  ]

class BITMAPINFOHEADER(ctypes.Structure):
  _fields_ = [
    ("biSize",          wintypes.DWORD),
    ("biWidth",         wintypes.LONG),
    ("biHeight",        wintypes.LONG),
    ("biPlanes",        wintypes.WORD),
    ("biBitCount",      wintypes.WORD),
    ("biCompression",   wintypes.DWORD),
    ("biSizeImage",     wintypes.DWORD),
    ("biXPelsPerMeter", wintypes.LONG),
    ("biYPelsPerMeter", wintypes.LONG),
    ("biClrUsed",       wintypes.DWORD),
    ("biClrImportant",  wintypes.DWORD),
  ]

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
dwmapi = ctypes.windll.dwmapi
buffer_size = WIDTH * HEIGHT * 4
width = wintypes.LONG(WIDTH)
height = wintypes.LONG(HEIGHT)
neg_height = wintypes.LONG(-HEIGHT)
hwnd = ctypes.windll.user32.FindWindowW(None, TITLE)
rect = RECT()
dwmapi.DwmGetWindowAttribute(
  wintypes.HWND(hwnd),
  wintypes.DWORD(9),
  ctypes.byref(rect),
  ctypes.sizeof(rect)
)
hdc_window = user32.GetWindowDC(hwnd)
new = True

UP = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_UP
DOWN = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_DOWN
LEFT = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_LEFT
RIGHT = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_RIGHT
START = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_START
BACK = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_BACK
LS = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_LEFT_THUMB
RS = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_RIGHT_THUMB
LT = 0x10000
RT = 0x20000
LB = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_LEFT_SHOULDER
RB = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_RIGHT_SHOULDER
GUIDE = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_GUIDE
A = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_A
B = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_B
X = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_X
Y = vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_Y

def shot(save_path=None):
  hdc_mem = gdi32.CreateCompatibleDC(hdc_window)
  hbitmap = gdi32.CreateCompatibleBitmap(hdc_window, width, height)
  gdi32.SelectObject(hdc_mem, hbitmap)
  user32.PrintWindow(hwnd, hdc_mem, 3)

  bmp_header = BITMAPINFOHEADER()
  bmp_header.biSize = ctypes.sizeof(BITMAPINFOHEADER)
  bmp_header.biWidth = width
  bmp_header.biHeight = neg_height
  bmp_header.biPlanes = 1
  bmp_header.biBitCount = 32
  bmp_header.biCompression = 0

  buffer = (ctypes.c_ubyte * buffer_size)()
  gdi32.GetDIBits(hdc_mem, hbitmap, 0, height, buffer, ctypes.byref(bmp_header), 0)

  img_data = numpy.frombuffer(buffer, dtype=numpy.uint8).reshape((HEIGHT, WIDTH, 4))
  img_data = img_data[:, :, [2, 1, 0, 3]]
  img = Image.fromarray(img_data, 'RGBA').convert('RGB')

  gdi32.DeleteObject(hbitmap)
  gdi32.DeleteDC(hdc_mem)

  if save_path:
    img.save(save_path)
  return img

def press(button, hold_time=0.1):
  if button == LT:
    PAD.left_trigger(255)
  elif button == RT:
    PAD.right_trigger(255)
  else:
    PAD.press_button(button=button)
  PAD.update()
  time.sleep(hold_time)

def release(button, post_sleep=0.1):
  if button == LT:
    PAD.left_trigger(0)
  elif button == RT:
    PAD.right_trigger(0)
  else:
    PAD.release_button(button=button)
  PAD.update()
  time.sleep(post_sleep)

def click(button, hold_time=0.1, post_sleep=0.1):
  press(button, hold_time)
  release(button, post_sleep)

def lstick(x=0, y=0, duration=0.0):
  PAD.left_joystick(x, y)
  PAD.update()
  time.sleep(duration)

def rstick(x=0, y=0, duration=0.0):
  PAD.right_joystick(x, y)
  PAD.update()
  time.sleep(duration)

class Task:
  def run(self):
    PAD.update()

  def reset(self):
    PAD.reset()
    PAD.update()

def ease(img):
  yellow = {'r': (235, 255), 'g': (235, 255), 'b': (150, 170)}
  cyan = {'r': (40, 60), 'g': (205, 225), 'b': (170, 190)}
  yp = []
  cp = []
  pix = img.load()
  for x in range(407, 880):
    r, g, b = pix[x, 50]
    if (yellow['r'][0] <= r <= yellow['r'][1] and
      yellow['g'][0] <= g <= yellow['g'][1] and
      yellow['b'][0] <= b <= yellow['b'][1]):
      yp.append(x)
    if (cyan['r'][0] <= r <= cyan['r'][1] and
      cyan['g'][0] <= g <= cyan['g'][1] and
      cyan['b'][0] <= b <= cyan['b'][1]):
      cp.append(x)
  if yp and cp:
    return (min(yp) + max(yp)) / 2, (min(cp) + max(cp)) / 2
  else:
    return (None, None)

class FISH(Task):
  timeout = 60

  def run(self):
    while True:
      start = time.time()
      def is_timeout():
        return (time.time() - start) > self.timeout

      while True:
        if is_timeout():
          break
        img = shot()
        p = img.getpixel((763, 295))
        if all(c > 245 for c in p):
          click(B)
          continue
        p = img.getpixel((657, 213))
        if all(190 < c < 210 for c in p):
          click(B)
          continue
        p = img.getpixel((1019, 625))
        if all(205 < c < 225 for c in p):
          click(Y, 0.1, 0.5)
          click(A)
          continue
        p = img.getpixel((1192, 653))
        if any(c < 252 for c in p):
          time.sleep(0.1)
          continue
        p = img.getpixel((1159, 648))
        if p[2] < 252:
          click(A)
          break
      if is_timeout():
        click(B, 0.1, 1)
        continue
      while True:
        if is_timeout():
          break
        img = shot()
        p = img.getpixel((1159, 648))
        if p[2] > 252:
          click(A, 0.1, 0.5)
          break
        else:
          time.sleep(0.1)
          continue
      if is_timeout():
        click(B, 0.1, 1)
        continue
      key = None
      while True:
        if is_timeout():
          break
        img = shot()
        y, c = ease(img)
        if y is None:
          release(LT, 0)
          release(RT, 0.5)
          break
        if abs(y-c) < 11:
          continue
        if y > c and (not key == LT):
          release(RT, 0)
          press(LT, 0)
          key = LT
        if y < c and (not key == RT):
          release(LT, 0)
          press(RT, 0)
          key = RT
        time.sleep(0.05)
      if is_timeout():
        click(B, 0.1, 1)
        continue
      while True:
        if is_timeout():
          break
        img = shot()
        p = img.getpixel((647, 317))
        if all(c > 245 for c in p):
          click(B)
          break
        else:
          time.sleep(0.1)
          continue

if __name__ == "__main__":
  parser = argparse.ArgumentParser()
  parser.add_argument("task", type=str)
  args = parser.parse_args()
  if args.task == 'shot':
    shot('shot.png')
  else:
    global PAD
    PAD = vgamepad.VX360Gamepad()
    TaskClass = globals().get(args.task.upper())
    task = TaskClass()
    time.sleep(1)
    task.run()
