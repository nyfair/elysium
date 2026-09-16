fn base_char() {
    #{
        skip1: false,
        attack1: || { click(RT, 0.1, 0.3); },
        skip2: false,
        attack2: || {
            click(X, 0.1, 0.2);
            click(X, 0.1, 0.3);
            click(X, 0.1, 0.6);
            click(X);
        },
    }
}

fn sagiri() {
    base_char() + #{
        skip2: true,
    }
}

fn lacrimosa() {
    base_char() + #{
        attack1: || { click(RT, 0.1, 1.2); },
        attack2: || {
            click(X, 0.1, 0.2);
            click(X, 0.1, 0.3);
            click(X, 0.1, 0.4);
            click(X, 0.1, 0.7);
            click(X);
        },
    }
}

fn nanally() {
    base_char() + #{
        attack1: || {
            click(RT, 0.1, 1.2);
            click(X, 0.1, 0.2);
            click(X, 0.1, 0.3);
            click(X, 0.1, 0.4);
            click(X, 0.1, 0.4);
            click(X);
        },
        attack2: || {
            click(X, 0.1, 0.2);
            click(X, 0.1, 0.3);
            click(X, 0.1, 0.4);
            click(X, 0.1, 0.4);
            click(X);
        }
    }
}

fn mint() {
    base_char() + #{
        attack2: || { click(RT, 0.1, 0.3); },
    }
}

fn haniel() {
    base_char() + #{
        skip2: true,
    }
}

fn edgar() {
    base_char() + #{
        skip1: true,
        attack2: || { click(RT, 0.1, 0.3); },
    }
}

fn cang() {
    base_char() + #{
        attack1: || { click(RT, 0.6, 0.1); },
        attack2: || { click(X, 0.6, 0.1); },
    }
}

fn hathor() {
    base_char() + #{
        skip1: true,
        attack2: || { click(RT, 5.5, 0.1); },
    }
}

fn adler() {
    base_char() + #{
        skip2: true,
    }
}

fn zankou() {
    base_char() + #{
        attack1: || {
            click(X, 0.1, 0.3);
            click(X, 0.6, 0.6);
            click(RT, 0.1, 2.0);
            // click(X, 0.1, 0.3);
            // click(X, 0.1, 0.3);
            // click(X, 0.1, 0.3);
            // click(X, 0.1, 0.3);
            // click(X, 0.1, 0.3);
            // click(X, 0.6, 0.6);
            // click(RT, 0.1, 2.0);
        },
        attack2: || {
            click(X, 0.1, 0.3);
            click(X, 0.6, 0.6);
            click(RT, 0.1, 2.0);
        },
    }
}

fn fadia() {
    base_char() + #{
        skip2: true,
    }
}

fn jin() {
    base_char() + #{
        attack1: || { click(X, 0.4, 0.1); },
        attack2: || { click(X, 0.4, 0.1); },
    }
}

fn kuhara() {
    base_char() + #{
        attack1: || {
            click(RT, 0.1, 0.2);
            lstick(0, -30000, 0.5);
            click(RB, 0.1, 0.5);
            lstick();
            click(Y, 0.1, 4.5);
        },
    }
}

fn chaos() {
    base_char() + #{
        attack2: || {
            click(X, 0.1, 0.2);
            click(X, 0.1, 0.3);
            click(X, 0.1, 0.4);
            click(X, 0.1, 0.4);
            click(X);
        }
    }
}

fn radio() {
    base_char() + #{
        skip1: true,
        attack2: || { click(RT, 0.2, 3.5); },
    }
}

fn chiichan() {
    base_char() + #{
        attack1: || { click(RT, 2.5, 0.1); },
        skip2: true,
    }
}

fn oneiroi() {
    base_char() + #{
        skip2: true,
    }
}

fn shinku() {
    base_char() + #{
        attack2: || { click(RT, 0.1, 0.3); },
    }
}

fn init_chara(name) {
    switch name {
        "早雾" => sagiri(),
        "安魂曲" => lacrimosa(),
        "娜娜莉" => nanally(),
        // "翳" => base_char(),
        "薄荷" => mint(),
        "哈尼娅" => haniel(),
        "埃德嘉" => edgar(),
        "白藏" => cang(),
        "哈索尔" => hathor(),
        "阿德勒" => adler(),
        "残虹" => zankou(),
        "法帝娅" => fadia(),
        // "零" => base_char(),
        "浔" => jin(),
        // "达芙蒂尔" => base_char(),
        "九原" => kuhara(),
        // "海月" => base_char(),
        "卡厄斯" => chaos(),
        "灵可" => radio(),
        "小吱" => chiichan(),
        "伊洛伊" => oneiroi(),
        "真红" => shinku(),
        _ => base_char(),
    }
}

fn switch_to(idx) {
    let keys = [UP, RIGHT, DOWN, LEFT];
    click(keys[idx]);
}

fn do_turn(team, idx, cur, p) {
    let c = team[idx];
    let char = init_chara(c.name());
    if p % 2 == 0 {
        if !char.skip1 {
            switch_to(idx);
            char.attack1();
        } else {
            return idx;
        }
    } else {
        if !char.skip2 {
            switch_to(idx);
            char.attack2();
        } else {
            return idx;
        }
    }
    let f = find_treasure(shot());
    if f[0] != 0 && f[2] > 99 {
        print(`宝箱已出现 (${f[0]},${f[1]},${f[2]})`);
        return -1;
    }
    idx
}

fn battle(team) {
    let main = [];
    let burst = [];
    let rest = [];
    for i in 0..4 {
        let c = team[i];
        if c.tag().index_of("主力输出") != -1 {
            main.push(i);
        } else if c.tag().index_of("爆发输出") != -1 {
            burst.push(i);
        } else {
            rest.push(i);
        }
    }
    let order = main + burst + rest;
    let cur = 0;
    let p = 0;
    loop {
        for k in 0..order.len() {
            let idx = order[k];
            cur = do_turn(team, idx, cur, p);
            if cur < 0 {
                return;
            }
        }
        p += 1;
    }
}
