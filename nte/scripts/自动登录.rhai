// {
//   "author": "",
//   "desc": "自动登录并识别基础信息，启动游戏请使用launch子命令",
//   "loop": false,
//   "timeout": 3600
// }

wait(10.0);
loop {
    let img = shot();
    let t = ocr(img, 590, 595, 105, 50);
    if t.contains("进入游戏") {
        break
    } else {
        t = ocr(img, 34, 628, 1216, 69);
        print(t);
        wait(2.0);
    }
}
for i in 0..3 { click(A, 0.1, 0.3); }
loop {
    let img = shot();
    if (pixel_like(img, 32, 134, 255, 255, 255, 15) &&
        pixel_like(img, 902, 37, 255, 255, 255, 15) &&
        pixel_like(img, 1182, 37, 255, 255, 255, 15) &&
        !pixel_like(img, 26, 134, 255, 255, 255, 15)) {
        break
    }
    let t = ocr(img, 523, 66, 235, 90);
    if t.contains("异晶") {
        print(t);
        click(B, 0.1, 0.6);
        click(B, 0.1, 0.6);
        click(A, 0.1, 0.6);
        click(A, 0.1, 0.6);
        break
    }
    sleep(0.5);
}
click(BACK, 0.1, 1.7);
let img = shot();
let t1 = ocr(img, 869, 28, 100, 22);
let t2 = ocr(img, 1044, 28, 100, 22);
print(`都市活力 ${t1} | 本性像素 ${t2}`);
click(B);
wait_loading();
