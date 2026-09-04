// {
//   "author": "",
//   "desc": "灾厄半自动挂机",
//   "loop": false,
//   "timeout": 99999
// }

loop {
    let img = shot();
    if task_ended(img) {
        print("无尽任务完成");
        break;
    }
    if ((pixel_like(img, 500, 225, 25, 25, 25, 10)
        && pixel_like(img, 765, 225, 25, 25, 25, 10)
        && !pixel_like(img, 554, 260, 25, 25, 25, 10)
    ) || pixel_like(img, 500, 176, 25, 25, 25, 10)
        && pixel_like(img, 765, 176, 25, 25, 25, 10)
        && !pixel_like(img, 554, 211, 25, 25, 25, 10)
    ) {
        set_pause(true);
        if get_turn() >= TURN {
            click(X, 1.2, 6.0);
            print("达到最大轮数");
            break;
        } else {
            click(Y, 1.2, 6.0);
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
