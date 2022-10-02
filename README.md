# gmod-filestealer-rs

To compile you need the latest version of [rust](https://www.rust-lang.org/tools/install).

Clone the project, and open command prompt in the root of the directory then run
```cargo build --release```

The dll will be placed in
```gmod-filestealer-rs\target\release```

Run gmod in 64 bit then inject the dll into gmod.exe

All files are placed in the drive your gmod is located + /stealer/, for example if your gmod is located on your D: drive then it'll be placed in D:/stealer/
