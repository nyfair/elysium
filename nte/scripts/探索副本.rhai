// {
//   "author": "",
//   "desc": "boost代表花费的体力，strategy代表副本名字",
//   "loop": false,
//   "timeout": 1000
// }

import "nte/battle_system" as bs;

teleport("胡迪尼的魔术舞台");
click(RS);
lstick(0, 30000, 1.0);
lstick();
click(B, 0.1, 0.6);
click(A);
lstick(0, 30000, 3.0);
lstick();
let team = get_team(shot());
bs::init_team(team);
bs::battle(team);

let img = shot();
print(find_treasure(img));
