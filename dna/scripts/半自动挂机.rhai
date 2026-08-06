// {
//   "author": "",
//   "desc": "半自动无尽：手动进入任务，每一轮结束后自动加速进入下一轮",
//   "loop": false,
//   "timeout": 20000
// }
while !get_stop() {
    let img = shot();
    if (pixel_equal(img, 881, 655, 0, 0, 0)
        && pixel_equal(img, 1123, 655, 0, 0, 0)
        && !pixel_equal(img, 900, 681, 0, 0, 0)
    ) {
        set_stop(true);
        print("无尽任务完成");
        break;
    }
    if (pixel_equal(img, 544, 630, 0, 0, 0)
        && pixel_equal(img, 709, 630, 0, 0, 0)
        && !pixel_equal(img, 550, 631, 0, 0, 0)
    ) {
        set_pause(true);
        click("Y");
    }
    if (pixel_like(img, 381, 509, 30, 58, 50, 5)
        && pixel_like(img, 477, 509, 30, 58, 50, 5)
        && !pixel_like(img, 381, 535, 30, 58, 50, 5)
    ) {
        set_pause(true);
        if get_turn() >= TURN {
            set_stop(true);
            click("X", 1.2);
            print("达到最大轮数");
            break;
        } else {
            click("Y", 1.2);
            click("A", 0.1, 1.0);
            print(`完成第 ${get_turn()} 轮`);
        }
        set_turn(get_turn() + 1);
        set_pause(false);
    }
    if COMBO.len() > 0 {
        run_combo(COMBO);
    } else {
        sleep(0.5);
    }
}
