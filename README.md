# Solver for Fortune's Foundation

This is a solver for a really hard solitaire game from the [Zachtronics Solitaire Collection](https://www.zachtronics.com/solitaire-collection/), a really fun game.

This repo is in two parts:
- solsolver, written in Rust, solves game positions
- zacdetect, written in Python, uses computer vision to read the game state from the screen, and uses pyautogui to simulate mouse inputs to play the game