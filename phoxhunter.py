import ctypes
from ctypes import wintypes
from concurrent.futures import ThreadPoolExecutor
import time
from PIL import Image
import numpy
import vgamepad

TITLE = "二重螺旋  "
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
executor = ThreadPoolExecutor(max_workers=1)

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

def task_ended():
  img = shot()
  p = img.getpixel((1000, 32))
  if any(c > 15 for c in p):
    return False
  return True

class Task:
  timeout = 120

  def run(self):
    click(Y, 0.1, 5.5)
    print('new task')
    lstick(0, -10000, 0.8)
    lstick()
    press(LB)
    click(X)
    release(LB, 0.5)
    click(RT)
    press(LB)
    click(X, 0.1, 0.8)
    click(Y)
    release(LB, 5.5)
    press(LB)
    click(X)
    release(LB, 0.5)
    click(RT)
    click(RB, 0.1, 0.5)
    lstick(0, 10000, 0.7)
    click(RB, 0.1, 1.2)
    lstick()
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    lstick(0, -10000)
    press(LB)
    click(X)
    release(LB)
    click(RB, 0.1, 1)
    lstick(0, 0, 0.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 0.9)
    press(LB)
    click(Y)
    release(LB, 3)
    press(LB)
    click(Y)
    release(LB, 3.2)
    click(RB, 0.1, 1.2)
    click(RB, 0.1, 1.2)

def reset():
  PAD.reset()
  PAD.update()
  time.sleep(1)
  click(START, 0.1, 0.3)
  click(X, 0.1, 0.3)
  click(A)

def run(task):
  while True:
    status = task_ended()
    if status:
      time.sleep(2.5)
      task.run()
      return
    else:
      time.sleep(0.03)

PAD = vgamepad.VX360Gamepad()
# time.sleep(1)
task = Task()
new = True
while True:
  future = executor.submit(run, task)
  try:
    future.result(task.timeout * 2 if new else task.timeout)
    new = False
  except Exception as e:
    print(f'task reset: {e}')
    reset()
    new = True
