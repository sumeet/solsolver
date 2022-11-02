import io
import subprocess
import time
import multiprocessing
from collections import namedtuple

import PIL.Image
import glob
import os
from subprocess import getoutput, check_output

jmport pyautogui
import pyscreeze
import numpy as np
import cv2

window_id = int(getoutput('xdotool search --classname ZachtronicsSolitaire'.strip()))
# there are 10 starting stacks (and a stack in the middle, that should be skipped)
num_starting_cards_per_stack = 7
to_top_of_stacks_px = 392
stack_width_px = 133
card_top_height_px = 35
card_total_height_px = 207
card_image_height_px = 34
gaps_until_next_stack = [
    201,
    30,
    29,
    29,
    31,
    29,  # the gap
    29,
    31,
    28,
    30,
    29]


# grab images of all cards in all stacks (except the middle) -- should be 70 cards
def get_card_images(pil_image):
    card_images = []
    left_cursor = 0
    for (deck_index, gap) in enumerate(gaps_until_next_stack):
        left_cursor += gap
        for i in range(num_starting_cards_per_stack):
            if deck_index == 5:
                continue
            top_cursor = to_top_of_stacks_px + i * card_top_height_px
            card_images.append(pil_image.crop(
                (left_cursor, top_cursor, left_cursor + stack_width_px, top_cursor + card_image_height_px)))
        left_cursor += stack_width_px

    return card_images


# now unused code to generate the card_images files, which is already done
def save_all_cards_from_screen_to_disk(pil_image):
    card_images = get_card_images(pil_image)
    for i, card_image in enumerate(card_images):
        card_image.save(f'card_images/card{i}.png')


def find_best_matches_for_image(needle, haystack):
    needle = pyscreeze._load_cv2(needle)
    haystack = pyscreeze._load_cv2(haystack)
    heat_map = cv2.matchTemplate(haystack, needle, cv2.TM_CCOEFF_NORMED)
    locations = np.where(heat_map >= 0.8)
    matches = [(x, y, confidence) for (x, y, confidence) in zip(*locations[::-1], heat_map[locations])]
    return sorted(matches, key=lambda x: x[2], reverse=True)

# the same as find_best_matches_for_image except:
# if the card filename contains 'MAJ', then match using only the center 1/2 of the image
# if the filename does not contain 'MAJ', then match only the left 1/2 of the image
def find_best_matches_for_image_maj(needle_filename, haystack):
    needle = pyscreeze._load_cv2(needle_filename)
    haystack = pyscreeze._load_cv2(haystack)
    orig_needle = needle
    if 'MAJ' in needle_filename:
        # cut 3px off the top
        # cut 2px off the bottom
        # and only match the center 1/4 of the card
        needle = needle[3:-2, needle.shape[1] * 3 // 8:needle.shape[1] * 5 // 8]
    else:
        # only match the left 1/4 of the card, and shift to the right by 5px
        # extend to the right by 15px
        needle = needle[:, 5:(needle.shape[1] * 2 // 8) + 20]

    # cut 5px off the top and bottom of needle
    needle = needle[5:needle.shape[0] - 5, :]

    heat_map = cv2.matchTemplate(haystack, needle, cv2.TM_CCOEFF_NORMED)
    locations = np.where(heat_map >= 0.8)
    matches = [(x, y, confidence) for (x, y, confidence) in zip(*locations[::-1], heat_map[locations])]

    # if the card is a major, and we matched from the center, then restore the x position, making
    # it seem like we matched from the left edge
    if 'MAJ' in needle_filename:
        matches = [(x - (3*orig_needle.shape[1] // 8), y, confidence) for (x, y, confidence) in matches]

    # prevents us from thinking the score counter is actually a major card, they use the same font
    matches = (m for m in matches if m[0] >= 100 and m[1] >= 100)
    return sorted(matches, key=lambda x: x[2], reverse=True)

# same as locate_all_cards_on_screen, but prevents matching mistakes using the following heuristics:
# 1. the most confident card matches take precedence
# 2. if the location of a found card is within 5 square pixels of a previously found card, then continue to the next most confident location
def locate_all_cards_on_screen_heuristic(pil_image):
    all_locations = {}
    all_card_files = glob.glob('card_images/*.png')
    with multiprocessing.Pool() as pool:
        results = pool.starmap(find_best_matches_for_image_maj, ((cf, pil_image) for cf in all_card_files))

    card_names = [cf.split('/')[1].split('.')[0] for cf in all_card_files]
    results_with_card_names = zip(card_names, results)
    # sort results by the max confidence
    results_with_card_names = sorted(results_with_card_names, key=lambda x: x[1][0][2], reverse=True)

    for card_name, results in results_with_card_names:
        for x, y, confidence in results:
            if any((x - 5) < prev_x < (x + 5) and (y - 5) < prev_y < (y + 5) for prev_x, prev_y, _ in
                   all_locations.values()):
                continue
            all_locations[card_name] = (x, y, confidence)
            break

    return all_locations


window_geom = dict(line.split('=') for line in getoutput(f'xdotool getwindowgeometry --shell {window_id}').splitlines())
for key in window_geom:
    try:
        window_geom[key] = int(window_geom[key])
    except ValueError:
        pass

game_window_x_offset = window_geom['X']
game_window_y_offset = window_geom['Y']

TARGET_RESOLUTION = (2160, 1216)  # the resolution when i had it running on my second monitor

# HAXX
orig_window_size = None


def solve_screen():
    global orig_window_size

    xwd_bytes = os.popen(f'xwd -id {window_id}')

    pil_image = PIL.Image.open(io.BytesIO(check_output(['convert', 'xwd:-', 'png:-'], stdin=xwd_bytes)))

    # make sure pil_image dimensions and window_geom dimensions are within 5px of each other
    # (accounting for window borders)
    assert abs(pil_image.width - window_geom['WIDTH']) < 5
    assert abs(pil_image.height - window_geom['HEIGHT']) < 5

    orig_window_size = (pil_image.width, pil_image.height)

    # crop the image to 16:9, removing black bars on either the top/bottom or left/right
    width, height = pil_image.size
    if width / height > 16 / 9:
        # remove left/right
        pil_image = pil_image.crop((width / 2 - height * 16 / 9 / 2, 0, width / 2 + height * 16 / 9 / 2, height))
    else:
        # remove top/bottom
        pil_image = pil_image.crop((0, height / 2 - width * 9 / 16 / 2, width, height / 2 + width * 9 / 16 / 2))

    # scale pil_image to TARGET_RESOLUTION
    pil_image = pil_image.resize(TARGET_RESOLUTION)

    print('locating all cards on the screen...')
    all_cards_on_screen = locate_all_cards_on_screen_heuristic(pil_image)

    print(len(all_cards_on_screen))

    # validate all_cards_on_screen:
    # - there should be no duplicate cards
    # - and 70 cards in total
    assert len(all_cards_on_screen) == 70
    print(len(all_cards_on_screen.values()))
    print(len(set(all_cards_on_screen.values())))

    # print out all the duplicates in all_cards_on_screen
    for card, location in all_cards_on_screen.items():
        if list(all_cards_on_screen.values()).count(location) > 1:
            print(card, location)

    assert len(set(all_cards_on_screen.values())) == 70

    # organize the cards into stacks, based roughly on their x and y coordinates
    # cards that are roughly the same x coordinate are in the same stack, with increasing y coordinates
    stacks = []
    for card_name, (x, y, confidence) in all_cards_on_screen.items():
        for stack in stacks:
            if abs(stack[0][0] - x) < 10:
                stack.append((x, y, card_name))
                break
        else:
            stacks.append([(x, y, card_name)])

    # sort each stack by y coordinate
    for stack in stacks:
        stack.sort(key=lambda x: x[1])
    # sort all stacks by x coordinate
    stacks.sort(key=lambda x: x[0][0])

    # validate that each stack has the correct number of cards
    for i, stack in enumerate(stacks):
        assert len(stack) == 7, f'stack {i} has {len(stack)} cards: {stack}'

    stacks_str = ''
    for i, stack in enumerate(stacks):
        stacks_str += ','.join(card[2] for card in stack)
        if i == 4:
            stacks_str += '\n'
        stacks_str += '\n'

    stacks_str = stacks_str.strip()
    print('found the following stacks on screen:')
    print(stacks_str)

    # the solver bin is at ~/solsolver/target/release/solsolver
    # send stacks_str to it as stdin, and let's take a look at the output
    print('running solver...')
    return check_output(['../solsolver/target/release/solsolver'],
                        input=stacks_str.encode('utf-8')).decode('utf-8')


def parse_position(pos):
    return pos if pos == 'BLOCK' else tuple(map(int, pos.split(':')))


Move = namedtuple('Move', 'src dst num_sucks human_readable')


def parse_move(line):
    [move_str, sucks_str, human_readable] = line.split('@')
    [src, dst] = map(parse_position, move_str.split('-'))
    return Move(src=src, dst=dst, num_sucks=int(sucks_str),
                human_readable=human_readable)


BLOCK_POSITION_IN_GAME_SCREEN = (1661, 163)


def convert_stack_pos_to_game_screen_px(pos):
    if pos == 'BLOCK':
        return BLOCK_POSITION_IN_GAME_SCREEN
    (stack_number, depth) = pos
    x = sum(gaps_until_next_stack[0:stack_number + 1]) + stack_width_px * stack_number
    y = to_top_of_stacks_px + depth * card_top_height_px
    # offset by half the width of the stack, so we're grabbing by the center of the card
    x += stack_width_px / 2
    # and then offset by 1/2 of the card height, so we're grabbing somewhere in the center of the card
    y += card_total_height_px / 2
    return (x, y)


# write a function to convert game screen coordinates into entire desktop coordinates,
# accounting for the position of the window on the desktop, adding back in the black bars that
# were cropped out, and scaling up to the original resolution
def convert_game_screen_px_to_desktop_px(pos):
    (x, y) = pos
    (window_x, window_y) = (game_window_x_offset, game_window_y_offset)
    (window_width, window_height) = orig_window_size
    (target_width, target_height) = TARGET_RESOLUTION
    # account for the black bars that were cropped out
    if window_width / window_height > 16 / 9:
        # remove left/right
        window_x += (window_width - window_height * 16 / 9) / 2
        window_width = window_height * 16 / 9
    else:
        # remove top/bottom
        window_y += (window_height - window_width * 9 / 16) / 2
        window_height = window_width * 9 / 16
    # scale up to the original resolution
    x = window_x + x * window_width / target_width
    y = window_y + y * window_height / target_height
    return (x, y)


CLOSE_WIN_SCREEN_BUTTON_POS = (2095, 49)
NEW_GAME_BUTTON_POS = (872, 148)

while True:
    move_list_str = None
    try:
        move_list_str = solve_screen()
    except subprocess.CalledProcessError:
        print('solver failed, skipping this game and starting a new one')
        pyautogui.moveTo(*convert_game_screen_px_to_desktop_px(NEW_GAME_BUTTON_POS), duration=0.3)
        pyautogui.dragTo(*convert_game_screen_px_to_desktop_px(CLOSE_WIN_SCREEN_BUTTON_POS), duration=0.3)
        # and wait some seconds for the cards to be dealt
        time.sleep(6)
        continue

    moves = list(map(parse_move, move_list_str.strip().splitlines()))

    # first use xdotool to click the center of the window, to activate it
    window_center = (game_window_x_offset + orig_window_size[0] / 2, game_window_y_offset + orig_window_size[1] / 2)
    check_output(['xdotool', 'mousemove', str(window_center[0]), str(window_center[1])])
    check_output(['xdotool', 'click', '1'])

    print(f'solved in {len(moves)} moves')
    for move in moves:
        # sleep longer the more sucks there are
        sleep_time = 0.2 * (1 + (2 * move.num_sucks))

        print(f'{move.human_readable} ({move.num_sucks} sucks, sleep {sleep_time}s)')
        pyautogui.moveTo(*convert_game_screen_px_to_desktop_px(convert_stack_pos_to_game_screen_px(move.src)))
        pyautogui.dragTo(*convert_game_screen_px_to_desktop_px(convert_stack_pos_to_game_screen_px(move.dst)),
                         duration=0.3)
        print('sleeping for', sleep_time)
        time.sleep(sleep_time)

    # now we'll see the success screen. wait some seconds, then press the close button

    # grandma movement
    pyautogui.moveTo(*convert_game_screen_px_to_desktop_px(CLOSE_WIN_SCREEN_BUTTON_POS), duration=5)
    # pyautogui click doesn't work here for some reason, so let's try drag instead even though it doesn't make any sense
    pyautogui.dragTo(*convert_game_screen_px_to_desktop_px(NEW_GAME_BUTTON_POS), duration=0.3)
    # pyautogui click doesn't work here for some reason, so
    # we're just drag to an arbitrary position to simulate a click
    pyautogui.dragTo(window_center, duration=0.3)

    # and wait some seconds for the cards to be dealt
    time.sleep(6)
