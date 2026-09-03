// {
//   "author": "",
//   "desc": "水狩月人巅峰赛3艾达，建议720p120hz最低画质降低延迟",
//   "loop": true,
//   "timeout": 90
// }

fn task_ended(img) {
    pixel_like(img, 1000, 32, 0, 0, 0, 20)
}

click(Y, 0.1, 4.0);
while !task_started(shot()) { wait(0.2); }
wait(1.0);
press(LB, 0.6);
click(Y, 0.1, 2.5);
release(LB, 0.0);

lstick(0, 10000, 3.0);
lstick();
rstick(15000, 20000, 0.43);
rstick();
click(A, 0.1, 0.5);
click(LS, 0.1, 0.6);
click(X, 0.1, 0.5);
loop { run_combo("e0"); }
