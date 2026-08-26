// {
//   "author": "",
//   "desc": "驱离，避险，单轮扼守，各类单次历练",
//   "loop": true,
//   "timeout": 90
// }
click(Y, 0.1, 0.2);
for b in 0..BOOST { click(RIGHT); }
click(A);
click(A, 0.1, 4.0);
while !task_started(shot()) { wait(0.2); }
wait(1.0);
if STRATEGY.len() > 0 { run_combo(STRATEGY); }
if COMBO.len() > 0 { loop { run_combo(COMBO); } }
