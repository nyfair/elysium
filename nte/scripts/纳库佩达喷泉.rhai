// {
//   "author": "",
//   "desc": "默认许愿，BOOST为1捞硬币",
//   "loop": false,
//   "timeout": 86400
// }

click(BACK, 0.1, 1.7);
locate("纳库佩达之池");
click(A, 0.3, 0.2);
click(A, 0.3, 0.2);
click(B, 0.3, 0.2);
locate("绘空町电话亭2");
click(A, 0.3, 0.2);
click(A, 0.3, 0.2);
click(A, 0.3, 0.2);
click(A, 0.5, 4.0);
wait_loading();
lstick(30000, -20000, 2.2);
lstick();
click(RS, 0.1, 0.9);
lstick(0, 30000, 0.1);
let cx = 0;
loop {
    click(RB, 0.1, 0.4);
    click(RB, 0.1, 0.4);
    click(A, 0.1, 0.4);
    click(A, 0.1, 0.8);
    let f = find_line(shot(), 98, 70, 1052, 570, 255, 231, 99, 0, false, false, true, 10, 20);
    if f[2] > 14 {
        lstick();
        cx = f[0];
        print("进入公园");
        break
    }
}
loop {
    let v = abs(640 - cx);
    if v < 4 {
        v = 4;
    }
    if cx < 636 {
        rstick(-10500, 0, v * 0.001);
    } else if cx > 644 {
        rstick(10500, 0, v * 0.001);
    } else {
        rstick();
        break
    }
    rstick();
    cx = find_line(shot(), 98, 70, 1052, 570, 255, 231, 99, 0, false, false, true, 10, 20)[0];
}
lstick(0, 30000, 0.1);
loop {
    click(RB, 0.1, 0.4);
    click(A, 0.1, 0.6);
    let t = ocr(shot(), 796, 379, 100, 26);
    if t.contains("纳库佩达") {
        lstick();
        break
    }
}
click(B, 0.1, 3.0);
for b in 0..BOOST { click(DOWN); }
for i in 0..20 {
    click(A);
}
lstick(0, -30000, 1.0);
lstick();
