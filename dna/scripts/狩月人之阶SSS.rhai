// {
//   "author": "",
//   "desc": "火狩月人巅峰赛法露茜，建议720p+120hz最低画质降低延迟",
//   "loop": true,
//   "timeout": 90
// }

fn task_ended(img) {
    pixel_like(img, 1000, 32, 0, 0, 0, 20)
}

click(Y, 0.1, 4.0);
while !task_started(shot()) { wait(0.2); }
wait(1.0);
run_combo("w1j3");
rstick(15000, 30000, 0.5);
rstick();

click(LS, 0.04, 0.3);
for i in 0..12 {
    click(X, 0.04, 0.04);
}
lstick(0, 10000, 0.0);
click(RS, 0.04, 0.04);
press(LB, 0.04);
click(Y, 0.04, 0.8);
release(LB, 0.04);
for i in 0..25 {
    click(X, 0.04, 0.04);
}
loop {
    press(LB, 0.04);
    click(X, 0.04, 0.04);
    release(LB, 0.04);
    for i in 0..20 {
        click(X, 0.04, 0.04);
    }
}
