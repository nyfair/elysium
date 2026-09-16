# 二游自动化辅助工具

---

## 🌟 主要特性

- 以安全为本，不使用Windows消息队列对游戏本体发送键鼠指令，无内存读取，无文件修改
- 支持副屏幕，虚拟屏幕，远程桌面，全程后台运行
- 超低资源占用，没有笨重的依赖地狱

---

## 🛠️ 安装与使用

1. 安装手柄驱动 [ViGEmBus](https://github.com/nefarius/ViGEmBus/releases/download/v1.22.0/ViGEmBus_1.22.0_x64_x86_arm64.exe)
2. [下载地址](https://github.com/nyfair/elysium/releases/tag/latest)，`full` 为完全版，其余为单一游戏版
3. 程序本体解压后开箱即用，同时支持命令行跑批与直接启动并登录游戏

---

## 🎮 支持的游戏

### 1. 蓝色星原：旅谣（AP）
- 自动登录

### 2. 二重螺旋（DNA）
- 全自动扼守
- 半自动挂机
- 沉浸式剧场
- 灾厄挂机
- 狩月人之阶SSS
- 自动登录
- 通用任务(驱离，避险，各类单次历练任务循环)
- 钓鱼

### 3. 异环（NTE)
- 一咖舍
- 伊洛伊撸宠
- 异象家具
- 排球之星
- 探索副本
- 日活领取
- 海上钓客
- 纳库佩达喷泉
- 自动登录
- 闪避辅助
- 魔女之家

---

## 📁 目录结构说明

```text
├── ap/                     # 《蓝色星原：旅谣》预置脚本与资源
├── dna/                    # 《二重螺旋》预置脚本与资源
├── nte/                    # 《异环》预置脚本与资源
└── user-{game}-scripts/    # 用户自定义脚本与配置文件 ({game} 为对应游戏缩写)
```

---

## 🖼️ 预览

### tui模式
![1](https://github.com/user-attachments/assets/d3894887-d3a3-4140-af29-107ecc217a68)

### cli模式
![2](https://github.com/user-attachments/assets/f1acbcc0-bd1e-4b27-afa6-216deba09a5d)

---

## ❓ FAQ

1. **手柄操作影响我正在玩的游戏怎么办？**  
   使用同为ViGEmBus作者制作的 [HidHide](https://github.com/nefarius/HidHide)，把虚拟手柄针对你玩的游戏藏起来，挂机打游戏两不误

2. **为什么应用名和项目名不一样？**  
   应用白名单机制，而且截图的原理和该款应用一致

3. **为什么不同游戏文件大小差别巨大？**  
   ocr功能捆绑了PaddleOCRv6模型（18MB），不同游戏需要的assets资源也不一致
