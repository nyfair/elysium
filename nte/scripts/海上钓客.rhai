// {
//   "author": "",
//   "desc": "海上钓客",
//   "loop": false,
//   "timeout": 86400
// }

fn is_timeout(start, timeout) {
    timestamp() - start > timeout
}

fn min_max(arr) {
    let mn = arr[0];
    let mx = arr[0];
    for v in arr {
        if v < mn { mn = v; }
        if v > mx { mx = v; }
    }
    [mn, mx]
}

fn ease(img) {
    let yp = [];
    let cp = [];
    for x in 407..880 {
        let p = get_pixel(img, x, 50);
        if p[0] >= 235 && p[0] <= 255 && p[1] >= 235 && p[1] <= 255 && p[2] >= 150 && p[2] <= 170 {
            yp.push(x);
        }
        if p[0] >= 40 && p[0] <= 60 && p[1] >= 205 && p[1] <= 225 && p[2] >= 170 && p[2] <= 190 {
            cp.push(x);
        }
    }
    if yp.len() > 0 && cp.len() > 0 {
        let ym = min_max(yp);
        let cm = min_max(cp);
        [ (ym[0] + ym[1]) / 2, (cm[0] + cm[1]) / 2 ]
    } else {
        [ -1, -1 ]
    }
}

let timeout = 40.0;
loop {
    if get_stop() { break; }
    let start = timestamp();
    let casted = false;
    while !is_timeout(start, timeout) {
        if get_stop() { break; }
        let img = shot();
        if pixel_like(img, 763, 295, 255, 255, 255, 10) {
            click("B");
            continue;
        }
        if pixel_like(img, 657, 213, 200, 200, 200, 10) {
            click("B");
            continue;
        }
        if pixel_equal(img, 1019, 625, 213, 213, 213) {
            print("fish prepare");
            click("Y", 0.1, 0.5);
            continue;
        }
        if !pixel_like(img, 1192, 653, 255, 255, 255, 4) {
            sleep(0.1);
            continue;
        }
        let p = get_pixel(img, 1159, 648);
        if p[2] < 252 {
            print("fish cast");
            click("A", 0.5);
            click("A");
            casted = true;
            break;
        }
    }
    if !casted {
        click("B", 0.1, 1.0);
        continue;
    }

    while !is_timeout(start, timeout) {
        if get_stop() { break; }
        let img = shot();
        let p = get_pixel(img, 1159, 648);
        if p[2] > 252 {
            print("fish bite");
            click("A", 0.1, 0.5);
            break;
        } else {
            print("wait for bite");
            sleep(0.1);
        }
    }
    if is_timeout(start, timeout) {
        click("B", 0.1, 1.0);
        continue;
    }

    print("fish ease");
    let key = "";
    while !is_timeout(start, timeout) {
        if get_stop() { break; }
        let img = shot();
        let r = ease(img);
        let y = r[0];
        let c = r[1];
        if y < 0 {
            print("stop ease");
            release("LT", 0.0);
            release("RT", 0.5);
            break;
        }
        if (y - c).abs() < 11 {
            continue;
        }
        if y > c && key != "LT" {
            print("left");
            release("RT", 0.0);
            press("LT", 0.0);
            key = "LT";
        }
        if y < c && key != "RT" {
            print("right");
            release("LT", 0.0);
            press("RT", 0.0);
            key = "RT";
        }
        sleep(0.05);
    }
    if is_timeout(start, timeout) {
        click("B", 0.1, 1.0);
        continue;
    }

    let start = timestamp() - timeout + 10.0;
    while !is_timeout(start, timeout) {
        if get_stop() { break; }
        let img = shot();
        if pixel_like(img, 647, 317, 255, 255, 255, 10) {
            print("finish fish");
            sleep(2.5);
            click("B");
            break;
        } else {
            sleep(0.1);
        }
    }
    if is_timeout(start, timeout) {
        click("B");
    }
}
