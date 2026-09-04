// {
//   "author": "",
//   "desc": "每天三次抚摸宠物",
//   "loop": false,
//   "timeout": 90
// }

let img = shot();
let team = get_team(img);
let target = "伊洛伊";
if team[0].name() == target {
    click(UP);
} else if team[1].name() == target {
    click(RIGHT);
} else if team[2].name() == target {
    click(DOWN);
} else if team[3].name() == target {
    click(LEFT);
} else {
    print(`${target}不在队伍里`);
    exit();
}

teleport("绘空町维特海默塔");
lstick(-24000, -26000, 1.1);
lstick(0, 0, 0.2);
for i in 0..3 {
    let t = ocr(shot(), 780, 360, 64, 64);
    if t.contains("抚摸") {
        click(B, 0.1, 2.0);
        click(A, 0.1, 1.0);
        click(B, 0.1, 0.5);
        click(B, 0.1, 1.5);
    } else {
        print("没有宠物，请调整交通系统密度");
        break
    }
}
