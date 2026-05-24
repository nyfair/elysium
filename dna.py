import argparse
from concurrent.futures import ThreadPoolExecutor
import time
from common import *

TITLE = "二重螺旋  "
init_window(TITLE)
executor = ThreadPoolExecutor(max_workers=1)

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

class Task:
  timeout = 0
  loop = True

  def run(self):
    click(Y, 0.1, 0.2)
    for _ in range(BOOST):
      click(RIGHT)
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
      print(f'{template.replace("fish_", "")}:{score:.2f}')
      if score > 0.4:
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
  args = parser.parse_args()
  if args.task == 'shot':
    shot('shot.png')
  else:
    global BOOST, ASSETS
    BOOST = args.boost
    ASSETS = load_assets()
    TaskClass = globals().get(args.task.upper())
    task = TaskClass()
    new = True
    click(GUIDE)
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
