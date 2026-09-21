// {
//   "author": "",
//   "desc": "自动登录，启动游戏请使用obs64 launch az，需要full版ocr功能",
//   "loop": false,
//   "timeout": 3600
// }

// wait(10.0);
loop {
    let img = shot();
    let t = ocr(img, 580, 572, 132, 64);
    print(t);
    if t.contains("开始旅程") {
        break
    } else {
        t = ocr(img, 100, 643, 1080, 72);
        print(t);
        wait(2.0);
    }
}
click(Y);
