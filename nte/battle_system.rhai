fn base_char() {
    #{
        name: "通用角色",
        attack: || { click(RT, 0.1, 0.3) }
    }
}

fn nanally() {
    base_char() + #{
        name: "娜娜莉",
        attack: || {
            click(RT, 0.1, 0.3);
            for i in 0..8 {
                click(X);
            }
        }
    }
}

fn init_chara(name) {
    switch name {
        "娜娜莉" => nanally(),
        _ => base_char(),
    }
}

fn switch_to(idx) {
    let keys = [UP, RIGHT, DOWN, LEFT];
    click(keys[idx]);
}

fn do_turn(team, idx, cur) {
    if idx != cur {
        switch_to(idx);
        let f = find_treasure(shot());
        if f[0] != 0 && f[2] > 98 {
            print(`宝箱已出现 (${f[0]},${f[1]},${f[2]})`);
            return -1;
        }
    }
    let c = team[idx];
    let cfg = init_chara(c.name());
    cfg.attack();
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
    loop {
        for k in 0..order.len() {
            let idx = order[k];
            cur = do_turn(team, idx, cur);
            if cur < 0 {
                return;
            }
        }
    }
}
