from concurrent.futures import ThreadPoolExecutor
import time
from common import *

TITLE = "二重螺旋  "
init_window(TITLE)
executor = ThreadPoolExecutor(max_workers=1)

def task_ended():
  img = shot()
  p = img.getpixel((1000, 32))
  if any(c > 15 for c in p):
    return False
  return True

class Task:
  timeout = 90

  def run(self):
    click(Y, 0.1, 5)
    print('new task')
    press(LB)
    click(X)
    release(LB, 0.5)
    click(RT)
    press(LB)
    click(X, 0.1, 1.1)
    click(Y)
    release(LB, 0.5)
    click(RB)
    lstick(0, 10000, 1.5)
    click(RB, 0.1, 1.2)
    lstick(10000, 0)
    click(RB, 0.1, 1)
    for _ in range(3):
      lstick(-10000, 0)
      click(RB, 0.1, 1.5)
      lstick(10000, 0)
      click(RB, 0.1, 1.5)
    lstick()
    for _ in range(3):
      press(LB)
      click(X, 0.1, 0.5)
      click(Y)
      release(LB, 1.8)
      press(LB)
      click(Y)
      release(LB, 1.5)
      click(RB)
      for _ in range(4):
        lstick(-10000, 0)
        click(RB, 0.1, 1.5)
        lstick(10000, 0)
        click(RB, 0.1, 1.5)
    press(LB)
    click(Y)
    release(LB)
    lstick()

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

task = Task()
while True:
  future = executor.submit(run, task)
  try:
    future.result(task.timeout)
  except Exception as e:
    print(f'task reset: {e}')
    reset()
