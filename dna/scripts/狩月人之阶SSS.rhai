// {
//   "author": "",
//   "desc": "暗狩月人刷分",
//   "loop": true,
//   "timeout": 90
// }

fn task_ended() {
    let img = shot();
    let p = get_pixel(img, 1000, 32);
    !(p[0] > 15 || p[1] > 15 || p[2] > 15)
}

print("new task");
click("Y", 0.1, 5.0);
press("LB");
click("Y", 2.0);
click("X", 0.1, 1.0);
click("X", 1.3);
click("X", 1.0);
release("LB", 4.3);
click("X", 1.0, 1.0);
click("X", 1.0, 1.0);
click("X", 1.0, 1.0);
click("X", 1.0);
