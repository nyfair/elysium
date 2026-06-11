import argparse
from concurrent.futures import ThreadPoolExecutor
import time
from common import *

TITLE = '二重螺旋  '
init_window(TITLE)
executor = ThreadPoolExecutor(max_workers=2)

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
  p = img.getpixel((172, 695))
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
  wait = 0
  loop = True
  cross = {
    'w': (0, 10000),
    's': (0, -10000),
    'a': (-10000, 0),
    'd': (10000, 0),
  }
  cur_turn = 1
  stop_combo = False
  pause_combo = False

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

  def run_combo(self, combo):
    for s, num in zip(combo[::2], combo[1::2]):
      if self.stop_combo:
        break
      while self.pause_combo:
        time.sleep(0.1)
      c = int(num)
      if s == 'j':
        for _ in range(c):
          click(LS, 0.1, 1)
      elif s in self.cross:
        x, y = self.cross[s]
        lstick(x, y, c)
        lstick()
      elif s == 'q':
        press(LB)
        click(Y)
        if c > 0:
          release(LB, c)
        else:
          release(LB)
      elif s == 'e':
        press(LB)
        click(X)
        if c > 0:
          release(LB, c)
        else:
          release(LB)
      elif s == 'p':
        time.sleep(c)

  def loop_combo(self):
    while not self.stop_combo:
      self.run_combo(COMBO)

  def endless(self):
    img = shot()
    p = img.getpixel((886, 638))
    if not any(c > 1 for c in p):
      self.stop_combo = True
      print('finish endless quest')
      return False
    p = img.getpixel((658, 602))
    if not any(c > 5 for c in p):
      self.pause_combo = True
      click(Y)
    p = img.getpixel((835, 519))
    if (214 < p[0] < 236) and (169 < p[1] < 191) and (73 < p[2] < 95):
      self.pause_combo = True
      if self.cur_turn >= TURN:
        self.stop_combo = True
        click(X, 1.2)
        print(f'endless task reach maximum turn')
        return False
      else:
        click(Y, 1.2)
        click(A, 0.1, 1)
        print(f'finish turn {self.cur_turn}')
      self.cur_turn += 1
      self.pause_combo = False
    return True

class Q(Task):
  timeout = 60
  wait = 1

  def run(self):
    super().run()
    time.sleep(self.wait)
    if STRATEGY:
      super().run_combo(STRATEGY)
    if COMBO:
      executor.submit(self.loop_combo)

class Q40(Q):
  timeout = 80

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

class E65(Q):
  timeout = 100
  wait = 0

  def run(self):
    super().run()
    rstick(-30000, 28000, 1)
    rstick(0, 0, 0.5)
    press(LB)
    click(X)
    release(LB, 1.25)
    click(X)
    rstick(-23000, -12000, 1)
    rstick()
    lstick(0, 10000, 2)
    lstick()
    aim(ASSETS['track_point'], 320, 380, 640, 320, 530, 590)
    press(LB)
    click(X)
    release(LB, 0.8)
    click(X, 0.1, 0.8)
    rstick(0, -32000, 0.6)
    rstick()
    for _ in range(5):
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
      if score > 0.4:
        return score
      time.sleep(0.1)

  def run(self):
    click(A)
    ease = False
    for i in range(100):
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

class SEMI(Task):
  timeout = 20000
  loop = False

  def run(self):
    if COMBO:
      executor.submit(self.loop_combo)
    while super().endless():
      time.sleep(0.5)

class AUTO(Q):
  timeout = 20000

  def run(self):
    self.cur_turn = 1
    self.stop_combo = False
    self.pause_combo = False
    super().run()
    while super().endless():
      time.sleep(0.5)

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

if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('task', type=str)
  parser.add_argument('--boost', '-b', type=int, default=0)
  parser.add_argument('--strategy', '-s', type=str, default='')
  parser.add_argument('--combo', '-c', type=str, default='')
  parser.add_argument('--turn', '-t', type=int, default=99)
  parser.add_argument('--timeout', '-o', type=int, default=0)
  args = parser.parse_args()
  if args.task == 'shot':
    shot('shot.png')
  else:
    global BOOST, STRATEGY, COMBO, TURN, ASSETS
    BOOST = args.boost
    STRATEGY = args.strategy
    COMBO = args.combo
    TURN = args.turn
    ASSETS = load_assets()
    TaskClass = globals().get(args.task.upper())
    task = TaskClass()
    new = True
    click(LB)
    while task.loop or new:
      print('wait for next task')
      future = executor.submit(run, task)
      try:
        future.result(task.timeout if args.timeout == 0 else args.timeout)
        new = False
      except Exception as e:
        if task.loop:
          print(f'task reset: {e}')
          task.reset()
