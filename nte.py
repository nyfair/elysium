import argparse
import time
from common import *

TITLE = "异环  "
init_window(TITLE)
new = True

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
    TaskClass = globals().get(args.task.upper())
    task = TaskClass()
    task.run()
