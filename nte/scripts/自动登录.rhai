// {
//   "author": "",
//   "desc": "自动登录并识别基础信息，登录窗口后台无法运作，脚本结束前请勿进行操作",
//   "loop": false,
//   "timeout": 3600
// }

wait(10.0);
loop {
    let img = shot();
    let t = ocr(img, 590, 595, 105, 50);
    if (t.contains("进入游戏")) {
        break
    } else {
        t = ocr(img, 34, 628, 1216, 69);
        print(t);
        wait(2.0);
    }
}
for i in 0..3 { click(A, 0.1, 0.3); }
wait_loading();
click(BACK, 0.1, 1.7);
let img = shot();
let t1 = ocr(img, 869, 28, 100, 22);
let t2 = ocr(img, 1044, 28, 100, 22);
print(`都市活力 ${t1} | 本性像素 ${t2}`);
click(B, 0.1, 0.9)
