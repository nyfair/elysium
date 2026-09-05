// {
//   "author": "",
//   "desc": "boost代表花费的体力，strategy代表副本名字",
//   "loop": false,
//   "timeout": 1000
// }

import "nte/battle_system" as bs;

let stamina = BOOST;
if BOOST < 40 {
    print("未正确填写花费体力，按40处理");
    stamina = 40;
}
let team = get_team(shot());
for i in 0..4 {
    let c = team[i];
    print(`${c.name()}:${c.element()}  ${c.tag()}`);
    print(`${c.debug_info()}`);
}
const STAGES = [
    #{ target: "胡迪尼的魔术舞台", list: ["合订本", "万花筒", "硬币记"] },
    #{ target: "胡迪尼的诡计舞台", list: ["小心鸽子", "扑克茶会", "惊喜派对", "心电感应", "越狱艺术"] },
    #{ target: "泡影罐头工厂",     list: ["苹果核", "螺旋乐", "液态梦", "冷甜点", "戏剧芯"] },
    #{ target: "兔子洞",           list: ["钟表把戏", "雕塑展馆", "纬线织机", "守卫萝卜", "精神图腾", "轨道之夜"] }
];
let p = -1;
for stage in STAGES {
    p = stage.list.index_of(STRATEGY);
    if p != -1 {
        teleport(stage.target);
        break;
    }
}
if p == -1 {
    print(`未知副本 ${STRATEGY}`);
    exit();
}
click(RS);
lstick(0, 30000, 1.0);
lstick();
click(B, 0.1, 0.6);
for b in 0..p {
    lstick(0, -30000, 0.2);
    lstick();
}

while stamina > 0 {
    click(A);
    lstick(0, 30000, 9.0);
    lstick();
    bs::battle(team);
    pad_reset();
    let aim = false;
    loop {
        let img = shot();
        let t = ocr(img, 796, 379, 90, 26);
        if t.contains("领取") {
            lstick();
            break
        } else {
            if aim {
                lstick(0, 30000, 0.1);
                lstick();
            } else {
                let f = find_treasure(img);
                let cx = f[0];
                loop {
                    if cx != 0 {
                        let v = abs(640 - cx);
                        if v < 4 {
                            v = 4;
                        }
                        if f[1] > 600 {
                            v = 640;
                        }
                        if cx < 630 {
                            rstick(-10500, 0, v * 0.001);
                        } else if cx > 650 {
                            rstick(10500, 0, v * 0.001);
                        } else {
                            rstick();
                            aim = true;
                            break
                        }
                        rstick();
                        cx = find_treasure(shot())[0];
                    } else {
                        lstick(0, 30000, 0.1);
                        lstick();
                        break
                    }
                }
            }
        }
    }
    click(B, 0.1, 0.4);
    if stamina > 79 {
        click(Y, 0.1, 2.0);
        print("领取双倍奖励");
        stamina -= 80;
    } else {
        click(A, 0.1, 2.0);
        print("领取奖励");
        stamina -= 40;
    }
}
