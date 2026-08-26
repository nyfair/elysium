// {
//   "author": "6溯弗洛拉，建议720p120hz最低画质降低延迟",
//   "desc": "水狩月人刷分",
//   "loop": true,
//   "timeout": 90
// }

fn task_ended(img) {
    pixel_like(img, 240, 32, 0, 0, 0, 20)
}

fn wait_flower() {
    let img = shot();
    while !(pixel_like(img, 631, 600, 245, 245, 245, 50) || pixel_like(img, 632, 592, 245, 245, 183, 50)) {
        wait(0.01);
        img = shot();
    }
}

fn flower_burst() {
    wait_flower();
    click(X, 0.6, 0.4);
}

click(Y, 0.1, 4.0);
while !task_started(shot()) { wait(0.2); }
wait(1.0);
press(LB);
click(Y, 0.1, 2.5);
click(X, 0.1, 0.5);
click(X, 0.1, 0.5);
release(LB, 0.0);
lstick(0, 10000, 3.0);
lstick();
rstick(15000, 20000, 0.43);
rstick();

wait_flower();
click(A, 0.1, 0.5);
click(LS, 0.1, 0.6);
click(X, 0.1, 0.5);
click(X, 0.6, 0.4);
flower_burst();
flower_burst();

for i in 0..3 {
    press(LB);
    click(X);
    release(LB);
    lstick(0, -10000, 0.0);
    flower_burst();
    flower_burst();
    lstick();
    flower_burst();
}

press(LB);
click(X);
release(LB);
loop { flower_burst() }
