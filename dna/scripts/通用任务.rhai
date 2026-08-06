// {
//   "author": "",
//   "desc": "驱离，避险，单轮扼守",
//   "loop": true,
//   "timeout": 90
// }
click("Y", 0.1, 0.2);
for b in 0..BOOST { click("RIGHT"); }
click("A");
click("A", 0.1, 4.0);
while !task_started() { sleep(0.2); }
sleep(1.0);
if STRATEGY.len() > 0 { run_combo(STRATEGY); }
if COMBO.len() > 0 { while !get_stop() { run_combo(COMBO); } }
