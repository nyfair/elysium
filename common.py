import ctypes
from ctypes import wintypes
import json
import time
from PIL import Image
import numpy
from scipy.signal import fftconvolve
import vgamepad

WIDTH = 1280
HEIGHT = 720
PAD = vgamepad.VX360Gamepad()

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

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
dwmapi = ctypes.windll.dwmapi
buffer_size = WIDTH * HEIGHT * 4
width = wintypes.LONG(WIDTH)
height = wintypes.LONG(HEIGHT)
neg_height = wintypes.LONG(-HEIGHT)
hwnd = None
hdc_window = None

def init_window(title):
  global hwnd, hdc_window
  hwnd = user32.FindWindowW(None, title)
  user32.PostMessageW(hwnd, 0x0006, 1, 0)
  rect = RECT()
  dwmapi.DwmGetWindowAttribute(wintypes.HWND(hwnd), wintypes.DWORD(9), ctypes.byref(rect), ctypes.sizeof(rect))
  hdc_window = user32.GetWindowDC(hwnd)

def load_assets():
  with open('coco_annotations.json') as f:
    j = json.load(f)
  assets = {}
  scale = HEIGHT / 2160
  cats = {c['id']: c['name'] for c in j['categories']}
  imgs = {i['id']: i['file_name'] for i in j['images']}
  cache = {}
  for ann in j['annotations']:
    name = cats.get(ann['category_id'])
    path = imgs[ann['image_id']]
    if path not in cache:
      raw = Image.open(path)
      cache[path] = raw.resize((int(raw.width * scale), int(raw.height * scale)), Image.LANCZOS)
    x, y, w, h = [int(v * scale) for v in ann['bbox']]
    assets[name] = load_img(cache[path].crop((x, y, x + w, y + h)))
  return assets

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

def load_img(img):
  if isinstance(img, str):
    img = Image.open(img)
  img_np = numpy.array(img, dtype=numpy.float32)
  img_np /= 255.0
  img_np = numpy.transpose(img_np, (2, 0, 1))
  return img_np

def normalize(t):
  t = t.astype(numpy.float32)
  t_mean = numpy.mean(t)
  t_std = numpy.std(t)
  return (t - t_mean) / (t_std + 1e-8)

def ncc_match(main_img, find_img):
  main_img = numpy.mean(main_img, axis=0, keepdims=True)
  find_img = numpy.mean(find_img, axis=0, keepdims=True)
  main_norm = normalize(main_img)
  find_norm = normalize(find_img)
  main_2d = main_norm.squeeze()
  find_2d = find_norm.squeeze()
  heatmap = fftconvolve(main_2d, find_2d[::-1, ::-1], mode='valid')
  y_min, x_min = numpy.unravel_index(numpy.argmax(heatmap), heatmap.shape)
  h, w = find_img.shape[-2:]
  x = x_min + int(w / 2)
  y = y_min + int(h / 2)
  pixel_count = find_norm.size
  score = numpy.max(heatmap) / pixel_count
  return (x, y, score)

def ncc_match_accurate(main_img, find_img):
  main_img = numpy.mean(main_img, axis=0)
  find_img = numpy.mean(find_img, axis=0)
  find_centered = find_img - numpy.mean(find_img)
  find_sum_sq = numpy.sum(find_centered**2)
  correlation = fftconvolve(main_img, find_centered[::-1, ::-1], mode='valid')
  ones = numpy.ones_like(find_img)
  local_sum = fftconvolve(main_img, ones, mode='valid')
  local_sum_sq = fftconvolve(main_img**2, ones, mode='valid')
  n = find_img.size
  local_energy = local_sum_sq - (local_sum**2) / n
  local_energy = numpy.clip(local_energy, 0, None)
  denominator = numpy.sqrt(local_energy * find_sum_sq)
  heatmap = numpy.divide(correlation, denominator, out=numpy.zeros_like(correlation), where=denominator > 1e-6)
  score = numpy.max(heatmap)
  y_min, x_min = numpy.unravel_index(numpy.argmax(heatmap), heatmap.shape)
  h, w = find_img.shape
  x = x_min + int(w / 2)
  y = y_min + int(h / 2)
  return (x, y, score)

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
