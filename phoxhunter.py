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
    click(Y, 2)
    click(X, 0.1, 1)
    click(X, 1.3)
    click(X, 1)
    release(LB, 22.3)
    press(X, 1)

def reset():
  PAD.reset()
  PAD.update()
  time.sleep(1)
  click(START, 0.1, 0.3)
  click(X, 0.1, 0.3)
  click(A, 0.1, 3)
  click(Y)

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
