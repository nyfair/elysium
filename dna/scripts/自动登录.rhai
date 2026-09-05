// {
//   "author": "",
//   "desc": "自动登录，启动游戏请使用elysium dna launch，需要full版ocr功能",
//   "loop": false,
//   "timeout": 3600
// }

wait(10.0);
loop {
    let img = shot();
    let t = ocr(img, 550, 600, 180, 90);
    if t.contains("进入游戏") {
        click(A);
        click(A);
        break
    } else {
        wait(0.5);
    }
}
