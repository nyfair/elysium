let img = shot();
let team = get_team(img);
print(team[0].name());
print(team[0].element());
print(team[0].tag());
print(team[0].debug_info());
print(team[1].name());
print(team[1].element());
print(team[1].tag());
print(team[1].debug_info());
print(team[2].name());
print(team[2].element());
print(team[2].tag());
print(team[2].debug_info());
print(team[3].name());
print(team[3].element());
print(team[3].tag());
print(team[3].debug_info());
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

travel("绘空町维特海默塔");
lstick(-24000, -26000, 1.1);
lstick();
for i in 0..3 {
    let t = ocr(shot(), 780, 360, 64, 64);
    if t.contains("抚摸") {
        click(B, 0.1, 2.0);
        click(A, 0.1, 1.0);
        click(B, 0.1, 0.5);
        click(B, 0.1, 1.5);
    }
}
