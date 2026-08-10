// {
//   "author": "",
//   "desc": "全自动无尽：自动开始任务并在每一轮结束后加速进入下一轮",
//   "loop": true,
//   "timeout": 20000
// }
click(Y, 0.1, 0.2);
for b in 0..BOOST { click(RIGHT); }
click(A);
click(A, 0.1, 4.0);
while !task_started() { wait(0.2); }
wait(1.0);

let img = shot();
if pixel_like(img, 632, 303, 255, 255, 200, 10) {
    run_combo("w1j4");
    rstick(-14000, 0, 0.1);
} else if pixel_like(img, 639, 315, 255, 255, 200, 10) {
    run_combo("w1j2");
    rstick(14000, 0, 0.1);
}
rstick();
lstick(0, 10000, 0.2);
lstick();

if STRATEGY.len() > 0 { run_combo(STRATEGY); }
loop {
    let img = shot();
    if (pixel_equal(img, 881, 655, 0, 0, 0)
        && pixel_equal(img, 1123, 655, 0, 0, 0)
        && !pixel_equal(img, 900, 681, 0, 0, 0)
    ) {
        print("无尽任务完成");
        break;
    }
    if (pixel_like(img, 381, 509, 30, 58, 50, 5)
        && pixel_like(img, 477, 509, 30, 58, 50, 5)
        && !pixel_like(img, 381, 535, 30, 58, 50, 5)
    ) {
        set_pause(true);
        if get_turn() >= TURN {
            click(X, 1.2);
            print("达到最大轮数");
            break;
        } else {
            click(Y, 1.2);
            click(A, 0.1, 1.0);
            print(`完成第 ${get_turn()} 轮`);
        }
        set_turn(get_turn() + 1);
        set_pause(false);
    }
    if COMBO.len() > 0 {
        run_combo(COMBO);
    } else {
        wait(0.5);
    }
}
