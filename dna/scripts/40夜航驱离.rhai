// {
//   "author": "",
//   "desc": "赛琪主控，防止漏捡，强烈推荐720p+30fps保证空中精细定位并降低游戏开销",
//   "loop": true,
//   "timeout": 90
// }
click(Y, 0.1, 0.2);
for b in 0..BOOST { click(RIGHT); }
click(A);
click(A, 0.1, 4.0);
while !task_started(shot()) { wait(0.2); }
wait(1.0);

click(LS, 0.1, 1.0);
rstick(-14000, 19000);
click(LS, 0.1, 0.4);
press(LB);
click(Y);
release(LB, 1.4);
rstick();
lstick(0, 10000, 3.3);
lstick();

let p = get_pixel(shot(), 1000, 500);
if p[0] > 200 || p[1] - p[0] > 50 {
    lstick(0, 10000, 5.0);
    lstick();
}
