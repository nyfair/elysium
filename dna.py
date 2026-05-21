import argparse
import ctypes
from ctypes import wintypes
from concurrent.futures import ThreadPoolExecutor
import json
import time
from PIL import Image
import numpy
from scipy.signal import fftconvolve
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

def short_aim_move(x, y=0, move_time=0.035, zero_time=0.01):
  PAD.right_joystick(x, y)
  PAD.update()
  time.sleep(move_time)
  PAD.right_joystick(0, 0)
  PAD.update()
  time.sleep(zero_time)

def aim(find_img, crop_l, crop_u, crop_w, crop_h, y_min, y_max, min_diff=10, max_diff=60):
  success = False
  max_try = 100
  cur_try = 0
  while not success:
    cur_try += 1
    if cur_try > max_try:
      raise Exception('aim failed')
    img = shot()
    cropped = load_img(img.crop((crop_l, crop_u, crop_l + crop_w, crop_u + crop_h)))
    x, y, score = ncc_match(cropped, find_img)
    x += crop_l
    y += crop_u
    print(f'x:{x} y:{y} score:{score:.2f}')
    target_x = WIDTH / 2
    offset = x - target_x
    if abs(offset) > max_diff:
      raise Exception('aim failed')
    if abs(offset) <= min_diff:
      if not y in range(y_min, y_max):
        lstick(0, 25000, 0.1)
        lstick()
      else:
        success = True
    else:
      n = 32750 if x > target_x else -32750
      short_aim_move(n)
  PAD.right_joystick(0, 0)
  PAD.update()

def task_started():
  img = shot()
  p = img.getpixel((172, 696))
  if any(c < 252 for c in p):
    return False
  return True

def task_ended():
  img = shot()
  p = img.getpixel((886, 638))
  if any(c > 3 for c in p):
    return False
  return True

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

class Task:
  timeout = 0
  loop = True

  def run(self):
    click(Y, 0.1, 0.2)
    for _ in range(BOOST):
      click(vgamepad.XUSB_BUTTON.XUSB_GAMEPAD_DPAD_RIGHT)
    click(A)
    click(A, 0.1, 4)
    started = False
    while not started:
      started = task_started()
      time.sleep(0.2)

  def reset(self):
    PAD.reset()
    PAD.update()
    time.sleep(1)
    click(START, 0.1, 0.3)
    click(X, 0.1, 0.3)
    click(A)

class Q40(Task):
  timeout = 60

  def run(self):
    super().run()
    click(LS, 0.1, 1)
    rstick(-14000, 19000)
    click(LS, 0.1, 0.4)
    press(LB)
    click(Y)
    release(LB, 1.4)
    rstick()
    lstick(0, 10000, 3.3)
    lstick()
    img = shot()
    p = img.getpixel((1000, 500))
    if p[0] > 200 or p[1] - p[0] > 50:
      lstick(0, 10000, 5)
      lstick()

class Q60(Task):
  timeout = 60

  def run(self):
    super().run()
    time.sleep(2.5)
    for _ in range(2):
      click(LS, 0.1, 1)
    press(LB)
    click(Y)
    release(LB)

class E65(Task):
  timeout = 100

  def run(self):
    super().run()
    rstick(-30000, 28000, 1)
    rstick(0, 0, 0.5)
    press(LB)
    click(X)
    release(LB, 1.25)
    click(RB)
    rstick(-23000, -12000, 1)
    rstick()
    lstick(0, 10000, 2)
    lstick()
    aim(ASSETS['track_point'], 320, 380, 640, 320, 530, 590)
    press(LB)
    click(X)
    release(LB, 0.8)
    click(RB, 0.1, 0.8)
    rstick(0, -32000, 0.6)
    rstick()
    for _ in range(6):
      click(LS, 0.1, 1)
    lstick(0, 10000, 1)
    lstick()
    press(LB)
    click(Y)
    release(LB)

class FISH(Task):
  timeout = 7200
  loop = False

  def get_score(self, template):
    img = shot()
    cropped = load_img(img.crop((1070, 540, 1190, 660)))
    _, _, score = ncc_match(cropped, ASSETS[template])
    return score

  def wait_condition(self, template):
    while True:
      score = self.get_score(template)
      print(f'{template.replace('fish_', '')}:{score:.2f}')
      if score > 0.5:
        return score
      time.sleep(0.1)

  def run(self):
    ease = False
    for i in range(1, 101):
      if not ease:
        self.wait_condition('fish_ease')
        click(A, 0.1, 1)
      self.wait_condition('fish_bite')
      time.sleep(3)
      self.wait_condition('fish_ease')
      click(A, 0.1, 4)
      score = 0
      while score < 0.2:
        score = self.get_score('fish_ease')
        print(f'wait:{score:.2f}')
        click(A, 0.1, 0.2)
      score = self.get_score('fish_chance')
      print(f'chance:{score:.2f}')
      if score > 0.7:
        click(A, 0.1, 1)
        ease = True
      else:
        time.sleep(2)
        ease = False

def run(task):
  if task.loop:
    while True:
      status = task_ended()
      if status:
        time.sleep(2.5)
        task.run()
        return
      else:
        time.sleep(0.5)
  else:
    task.run()

if __name__ == "__main__":
  parser = argparse.ArgumentParser()
  parser.add_argument("task", type=str)
  parser.add_argument("--boost", "-b", type=int, default=0)
  parser.add_argument("--wait", "-w", type=float, default=0)
  args = parser.parse_args()
  if args.task == 'shot':
    shot('shot.png')
  else:
    global BOOST, ASSETS, PAD
    BOOST = args.boost
    ASSETS = load_assets()
    PAD = vgamepad.VX360Gamepad()
    TaskClass = globals().get(args.task.upper())
    task = TaskClass()
    new = True
    if args.wait > 0:
      time.sleep(args.wait)
      click(A)
    while task.loop or new:
      print('new task')
      future = executor.submit(run, task)
      try:
        future.result(task.timeout)
        new = False
      except Exception as e:
        if task.loop:
          print(f'task reset: {e}')
          task.reset()
