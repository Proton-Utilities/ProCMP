# ProCMP

A build composer system using Lune & darklua with automated release deployment.

## About

ProCMP allows for easy build composition with composer-markers, it gives you access to runtime build data and add extra information such as headers to your distribution file.

| Release build | Debug build |
|-|-|
| ![Release build](./Assets/releasePreview.gif) | ![Debug build](./Assets/debugPreview.gif) |

## Dependencies

To use ProCMP, you must have Lune and Darklua installed. See the guides below for more information.

> It is most recommended to use [Aftman](https://github.com/LPGhatguy/aftman) to manage these dependencies.

- [Lune installation](<https://lune-org.github.io/docs/getting-started/1-installation>)
- [Darklua installation](<https://github.com/seaofvoices/darklua>)

## Usage

1. **Locate your lune folder**, it should be at the root of your directory, named ".lune" or simply "lune", if there is none, you can simply create one.

2. **Download the ProCMP script, and place it in this directory**. You can name it whatever you want, for simplicity, I will use "build.luau". If everything was done right, you should be able to use the `lune list` command and your script should show. </br> [Get the latest release](https://github.com/Proton-Utilities/ProCMP/releases/latest/download/build.luau)

3. **Add a frame**, this is essentially your build insertion file. Add composer markers to get build info like as the build itself, and the build version. </br> [Example frame](Example/build/frame.luau)

4. **Run using `lune run build <config_location>`**, you should be prompted with a CLI interface asking for the build configuration and version. After completing the prompt your file will be built and composed at the output location. </br> [Example config](Example/build/.pcmp.json)

> You can also use *VS code tasks* to build using a keybind instead of typing a terminal command </br> [Learn more](https://code.visualstudio.com/docs/debugtest/tasks)
