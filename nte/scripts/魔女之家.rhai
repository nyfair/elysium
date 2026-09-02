// {
//   "author": "",
//   "desc": "魔女之家Buff",
//   "loop": false,
//   "timeout": 90
// }

teleport("魔女之家");
click(RS, 0.1, 0.9);
lstick(0, 30000, 0.2);
click(RB, 0.1, 0.7);
click(RB, 0.1, 2.0);
lstick(0, 0, 6.2);
let t = ocr(shot(), 796, 379, 40, 26);
if t.contains("黑羽") {
    click(B, 0.1, 2.0);
    click(DOWN, 0.1, 0.3);
    click(A, 0.1, 1.5);
    click(A, 0.1, 1.5);
    click(X, 0.1, 5.0);
    click(B, 0.1, 1.0);
    click(B);
}
