// {
//   "author": "",
//   "desc": "不朽剧目高配芙罗拉，适配720p+120fps",
//   "loop": true,
//   "timeout": 600
// }

fn task_ended(img) {
    let p1 = get_pixel(img, 1139, 641);
    let p2 = get_pixel(img, 1130, 641);
    (p1[0] == 255 && p1[1] == 255 && p2[0] < 100 && p2[1] < 100) ||
    (pixel_equal(img, 934, 688, 0, 0, 0) && !pixel_equal(img, 873, 696, 0, 0, 0))
}

fn check() {
    let img = shot();
    if pixel_like(img, 491, 85, 0, 0, 0, 5) {
        return 1
    } else if pixel_equal(img, 425, 78, 0, 0, 0) {
        return 0
    }
    return 2
}

fn boss() {
    let img = shot();
    pixel_like(img, 406, 85, 0, 0, 0, 20) && !pixel_like(img, 405, 82, 0, 0, 0, 20)
}

let img = shot();
let p = get_pixel(img, 1139, 641);
if (p[0] == 255 && p[1] == 255) {
    click(X);
} else {
    click(Y);
}
print("new stage");
click(A, 0.1, 4.0);
while !task_started(shot()) { wait(0.2); }

rstick(20000, 0);
lstick(0, 10000, 0.35);
rstick(0, 0, 2.7);
lstick();
print("push device");
for i in 0..5 {
    click(Y);
    lstick(0, 10000, 0.1);
    lstick();
}

run_combo("q1s3e1e1");
let vitamin = 2;
loop {
    if vitamin < 1 {
        run_combo("e1");
        vitamin = 2;
    }
    switch check() {
        2 => {
          lstick(0, 10000);
          wait(0.2);
        },
        1 => {
            switch rand(4) {
                0 => lstick(-10000, 0),
                1 => lstick(10000, 0),
                2 => lstick(0, 10000),
                3 => lstick(0, -10000),
            }
            run_combo("l7L0");
            vitamin -= 1;
        },
        0 => break,
    }
}

print("boss stage");
pad_reset();
loop {
    if boss() {
        click(RS);
        if vitamin < 1 {
            run_combo("e1");
            vitamin = 2;
        }
        run_combo("l3L0");
        vitamin -= 1;
    } else {
        wait(0.1);
    }
}
