// {
//   "author": "",
//   "desc": "自动登录并识别基础信息，启动游戏请使用obs64 launch sp",
//   "loop": false,
//   "timeout": 3600
// }

wait(10.0);
loop {
    let img = shot();
    let t = ocr(img, 464, 316, 360, 216);
    if t.contains("START") {
        wait(1.0);
        click(A);
        break
    } else {
        t = ocr(img, 440, 606, 400, 100);
        for l in t.split('\n') {
            if l.len >= 2 &&
                !l.contains("著作权") &&
                !l.contains("批准文号") {
                print(l);
            }
        }
        wait(2.0);
    }
}
