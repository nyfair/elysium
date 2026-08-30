fn find_fountain(img, roi_x, roi_y, roi_w, roi_h, r, g, b) {
    let y = roi_y + roi_h;
    while y >= roi_y {
        let count = 0;
        let start_x = 0;
        let x = roi_x;
        while x <= roi_x + roi_w {
            if pixel_equal(img, x, y, r, g, b) {
                if count == 0 {
                    start_x = x;
                }
                count += 1;
            } else {
                if count >= 10 {
                    break
                }
                count = 0;
            }
            x += 1;
        }
        if count >= 10 {
            let center_x = start_x + count / 2;
            return [center_x, y, count]
        }
        y -= 1;
    }
    [0, 0, 0]
}

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
    let f = find_fountain(shot(), 98, 70, 1052, 570, 255, 231, 99);
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
    cx = find_fountain(shot(), 98, 70, 1052, 570, 255, 231, 99)[0];
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
click(B, 0.1, 0.4);
click(A, 0.1, 1.5);
click(A, 0.1, 1.5);
click(B, 0.1, 1.0);
click(B);
