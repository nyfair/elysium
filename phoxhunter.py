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
