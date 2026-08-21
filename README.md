**为什么已经有了 xxx，还会有这个？**
1. 不使用Windows消息队列对游戏本体进行键鼠指令发送，纯手柄硬件模拟，有效避免各种检测
2. 支持副屏幕和虚拟屏幕，全程后台运行，前台该干嘛干嘛，互不打扰
3. 超低的资源占用和依赖，没有笨重的耗电玩意

**安装与使用**
1. 先装手柄驱动 [ViGEmBus](https://github.com/nefarius/ViGEmBus/releases/download/v1.22.0/ViGEmBus_1.22.0_x64_x86_arm64.exe)
2. 程序本体开箱即用，同时支持命令行跑批

**预览**
<img alt="1" src="https://github.com/user-attachments/assets/d3894887-d3a3-4140-af29-107ecc217a68" />

**FAQ**  
1. **手柄操作影响我正在玩的游戏怎么办？**  
使用同为ViGEmBus作者制作的 [HidHide](https://github.com/nefarius/HidHide)，把虚拟手柄针对你玩的游戏藏起来，挂机打游戏两不误

2. **为什么应用名和项目名不一样？**  
应用白名单机制，而且截图的原理和该款应用一致
