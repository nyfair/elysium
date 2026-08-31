// {
//   "author": "",
//   "desc": "领取各种家具奖励",
//   "loop": false,
//   "timeout": 90
// }

click(BACK, 0.1, 1.7);
locate("绘空町", 2, 5);
lstick(30000, -25000, 0.07 * res_scale());
lstick();
click(A, 0.3, 0.2);
click(A, 0.3, 0.2);
click(A, 0.5, 4.);
wait_loading();
press(LB);
click(X);
release(LB, 2.0);
click(RB);
click(Y, 0.1, 2.5);
click(B, 0.1, 0.6);
click(B, 0.1, 0.6);
click(B);
