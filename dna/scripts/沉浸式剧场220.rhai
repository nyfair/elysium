// {
//   "author": "",
//   "desc": "不朽剧目高配挂机，boost 1=芙罗拉 2=伊薇 3=莉兹贝尔",
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
    let f = find_line(shot(), 409, 72, 150, 15, 0, 0, 0, 0, true, true, true, 20, 148);
    if f[1] > 77 {
        return 1
    } else if f[1] == 0 {
        return 0
    }
    return 2
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
for i in 0..3 {
    click(Y);
    lstick(0, 10000, 0.1);
    lstick();
}

switch BOOST {
    1 => run_combo("q3e1e1"),
    2 => run_combo("q3e1e1e1e1e1e1e1e1"),
    3 => {
        run_combo("q3");
        click(X, 6.5, 0.1);
    },
}
loop {
    switch check() {
        0 => {
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
            switch BOOST {
                1 => run_combo("e0L1"),
                2 => wait(1.0),
                3 => {
                    switch rand(2) {
                        0 => rstick(-30000, 0, 0.2),
                        1 => rstick(30000, 0, 0.2),
                    }
                    rstick();
                    run_combo("L1");
                },
            }
        },
        2 => break,
    }
}

print("boss stage");
pad_reset();
loop {
    if check() == 2 {
        click(RS);
        switch BOOST {
            1 => run_combo("e0L1"),
            2 => run_combo("e1w0"),
            3 => run_combo("L1w0"),
        }
    } else {
        pad_reset();
        wait(0.1);
    }
}
