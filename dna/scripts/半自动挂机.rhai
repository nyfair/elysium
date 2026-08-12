// {
//   "author": "",
//   "desc": "半自动无尽：手动进入任务，每一轮结束后自动加速进入下一轮",
//   "loop": false,
//   "timeout": 20000
// }
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
    if COMBO.len() > 0 {
        run_combo(COMBO);
    } else {
        wait(0.5);
    }
}
