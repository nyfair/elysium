// {
//   "author": "",
//   "desc": "悠闲循环钓鱼",
//   "loop": false,
//   "timeout": 7200
// }

fn get_score(template, x, y, w, h) {
    let m = ncc_match(shot(), template, x, y, w, h);
    m[2]
}

fn wait_condition(template, x, y, w, h, threshold) {
    loop {
        let score = get_score(template, x, y, w, h);
        print(`${template}:${score}`);
        if score > threshold {
            return;
        }
        wait(0.1);
    }
}

click("A");
let ease = false;
for i in 0..100 {
    if !ease {
        wait_condition("fish_ease", 1070, 540, 120, 120, 0.6);
        click("A", 0.1, 1.0);
    }
    wait_condition("fish_bite", 1070, 540, 120, 120, 0.4);
    wait(3.0);
    wait_condition("fish_ease", 1070, 540, 120, 120, 0.4);
    click("A", 0.1, 4.0);
    let score = 0.0;
    while score < 0.2 {
        score = get_score("fish_ease", 1070, 540, 120, 120);
        print(`fish_done:${score}`);
        click("A", 0.1, 0.2);
    }
    let chance = get_score("fish_chance", 1070, 540, 120, 120);
    print(`fish_luck:${chance}`);
    if chance > 0.7 {
        click("A", 0.1, 1.0);
        ease = true;
    } else {
        wait(2.0);
        ease = false;
    }
}
