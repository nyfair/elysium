// {
//   "author": "",
//   "desc": "全自动扼守：自动开始任务并在每一轮结束后加速进入下一轮",
//   "loop": true,
//   "timeout": 20000
// }

fn reset() {
    print("无法识别该副本");
    reset_task();
    exit();
}

click(Y, 0.1, 0.2);
for b in 0..BOOST { click(RIGHT); }
click(A);
click(A, 0.1, 4.0);
while !task_started(shot()) { wait(0.2); }
wait(1.0);

let img = shot();
let m = ncc_match(img, "track_point", 440, 160, 500, 300);
print(m);
if m[2] < 0.5 {
    reset();
} else if abs(m[0] - 632) < 2 && abs(m[1] - 307) < 2 {
    run_combo("w1j4");
    lstick(-10000, 10000, 0.05);
    lstick(0, 10000, 0.2);
    lstick();
} else if abs(m[0] - 639) < 2 && abs(m[1] - 318) < 2 {
    run_combo("w1j2");
    lstick(10000, 10000, 0.05);
    lstick(0, 10000, 0.1);
    lstick();
} else if abs(m[0] - 859) < 2 && abs(m[1] - 213) < 2 {
    run_combo("w1j1");
    rstick(0, 20000, 0.2);
    rstick();
    click(Y, 0.1, 4.0);
    lstick(10000, 0, 0.5);
    click(B, 0.1, 0.4);
    lstick();
} else if abs(m[0] - 662) < 2 && abs(m[1] - 226) < 2 {
    run_combo("w2j5");
    lstick(10000, 10000, 0.3);
    lstick();
} else if abs(m[0] - 634) < 2 && abs(m[1] - 299) < 2 {
    run_combo("w1j5");
} else if abs(m[0] - 635) < 2 && abs(m[1] - 302) < 2 {
    run_combo("w1j3");
} else if abs(m[0] - 666) < 2 && abs(m[1] - 359) < 2 {
    run_combo("w1j2");
    lstick(10000, 10000, 0.5);
    lstick(0, 10000, 0.2);
    lstick();
} else if abs(m[0] - 629) < 2 && abs(m[1] - 377) < 2 {
    run_combo("w1j4w1");
} else if abs(m[0] - 630) < 2 && abs(m[1] - 278) < 2 {
    run_combo("w2j3w1");
    lstick(0, 10000, 0.2);
    lstick();
} else if abs(m[0] - 586) < 2 && abs(m[1] - 399) < 2 {
    run_combo("w1j3");
    lstick(-10000, 10000, 0.7);
    lstick();
} else if abs(m[0] - 484) < 2 && abs(m[1] - 421) < 2 {
    run_combo("w1j3");
    lstick(-10000, 10000, 4.3);
    lstick();
    run_combo("j1a0");
} else {
    reset();
}

let started = false;
if STRATEGY.len() > 0 { run_combo(STRATEGY); }
loop {
    let img = shot();
    if task_ended(img) {
        print("无尽任务完成");
        break;
    }
    if (pixel_equal(img, 446, 265, 0, 0, 0)
        && pixel_equal(img, 629, 265, 0, 0, 0)
        && !pixel_equal(img, 544, 265, 0, 0, 0)
    ) {
        set_pause(true);
        click(Y);
    }
    if (pixel_equal(img, 681, 256, 255, 255, 255)
        && pixel_equal(img, 681, 288, 255, 255, 255)
        && !pixel_equal(img, 670, 256, 255, 255, 255)
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
    if !started && COMBO.len() > 0 {
        auto_combo(COMBO);
        started = true;
    }
    wait(0.5);
}
