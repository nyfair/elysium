// {
//   "author": "",
//   "desc": "自动排球等级/都市活力",
//   "loop": false,
//   "timeout": 86400
// }
fn check_receive(img) {
    !pixel_like(img, 1225, 300, 245, 245, 245, 10)
        && pixel_like(img, 1225, 388, 245, 245, 245, 10)
        && pixel_like(img, 1225, 417, 245, 245, 245, 10)
}

fn check_serve(img) {
    pixel_like(img, 1225, 300, 245, 245, 245, 10)
        && pixel_like(img, 1225, 328, 245, 245, 245, 10)
        && !pixel_like(img, 1225, 388, 245, 245, 245, 10)
}

fn check_spike(img) {
    pixel_like(img, 1101, 648, 203, 245, 251, 50)
        && pixel_like(img, 1163, 648, 203, 245, 251, 50)
        && !pixel_like(img, 1105, 648, 203, 245, 251, 50)
        && !pixel_like(img, 1093, 648, 203, 245, 251, 50)
}

fn check_start(img) {
    pixel_like(img, 1090, 686, 213, 213, 213, 10)
        && pixel_like(img, 1171, 686, 213, 213, 213, 10)
        && !pixel_like(img, 1090, 694, 213, 213, 213, 10)
}

fn check_end(img) {
    pixel_like(img, 50, 598, 232, 232, 232, 10)
        && pixel_like(img, 70, 598, 232, 232, 232, 10)
        && !pixel_like(img, 60, 598, 232, 232, 232, 10)
}

let last_serve = timestamp();
let last_receive = timestamp();
loop {
    let img = shot();
    if check_spike(img) {
        wait(0.2);
        while check_spike(shot()) { wait(0.01) }
        print("扣杀");
        wait(0.58);
        click(B, 0.1, 2.0);
    } else if check_serve(img) {
        if last_serve.elapsed > 3.5 {
            lstick(10000, 10000, 0.25);
            lstick();
            click(A, 0.1, 2.45);
            print("发球");
            click(B);
            last_serve = timestamp();
        }
        wait(0.1);
    } else if check_receive(img) {
        if last_receive.elapsed > 0.36 {
            switch rand(5) {
                4 => click(B),
                _ => click(A),
            }
            last_receive = timestamp();
        }
        wait(0.02);
    } else if check_start(img) {
        click(RS);
        click(RS);
        click(Y, 0.2, 5.0);
        click(Y, 0.2, 1.8);
    } else if check_end(img) {
        click(X, 0.2, 1.8);
        click(Y, 0.2, 1.8);
    } else {
        wait(0.2);
    }
}
