fn base_char() {
    #{
        name: "通用角色",
        attack: || { click(RT) }
    }
}

fn nanally() {
    base_char() + #{
        name: "娜娜莉",
        attack: || {
            click(RT, 0.1, 0.5);
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

fn init_team(team) {
    print("== 队伍识别 ==");
    for i in 0..4 {
        let c = team[i];
        print(`slot${i}: ${c.name()}  元素:${c.element()}  tag:${c.tag()}`);
    }
    team
}

fn switch_to(idx) {
    let keys = [UP, RIGHT, DOWN, LEFT];
    click(keys[idx], 0.1, 0.5);
    wait(0.3);
}

fn do_turn(team, idx, cur) {
    if idx != cur {
        switch_to(idx);
        let t = find_treasure(shot());
        if t[0] != 0 && t[2] > 98 {
            print(`宝箱已出现 (${t[0]},${t[1]},${t[2]})，停止战斗`);
            return -1;
        }
    }
    let c = team[idx];
    let cfg = init_chara(c.name());
    print(`${cfg.name} 攻击`);
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
    print(`战斗顺序: ${main} ${burst} ${rest}`);
    let cur = -1;
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
