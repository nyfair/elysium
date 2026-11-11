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

fn wait_condition(name, template, x, y, w, h, threshold) {
    loop {
        let score = get_score(template, x, y, w, h);
        print(`${name}:${score}`);
        if score > threshold {
            return;
        }
        wait(0.1);
    }
}

let ease_tpl = #{ file: "T_AngLishFish_Auto", size: [105, 105] };
let bite_tpl = #{ file: "T_Angling_Icon03", size: [105, 105] };
let chance_tpl = #{ file: "T_Angling_Icon05", size: [84, 84] };

click(A);
let ease = false;
for i in 0..100 {
    if !ease {
        wait_condition("ease", ease_tpl, 1070, 540, 120, 120, 0.6);
        click(A, 0.1, 1.0);
    }
    wait_condition("bite", bite_tpl, 1070, 540, 120, 120, 0.6);
    wait(3.0);
    wait_condition("ease", ease_tpl, 1070, 540, 120, 120, 0.6);
    click(A, 0.1, 4.0);
    let score = 0.0;
    while score < 0.2 {
        score = get_score(ease_tpl, 1070, 540, 120, 120);
        print(`done:${score}`);
        click(A, 0.1, 0.2);
    }
    let chance = get_score(chance_tpl, 1070, 540, 120, 120);
    print(`luck:${chance}`);
    if chance > 0.6 {
        click(A, 0.1, 1.0);
        ease = true;
    } else {
        wait(2.0);
        ease = false;
    }
}
