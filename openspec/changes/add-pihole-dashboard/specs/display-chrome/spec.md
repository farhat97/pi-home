# Spec Delta

## Purpose

Gives every app on the device the same btop-inspired look and a common header, so separately built apps read as one gadget on the 480x320 display.

## ADDED Requirements

### Requirement: Shared header
Every app SHALL render its name in the top border on the left, and on the right a rotation indicator with one marker per app (the active one highlighted) followed by the seconds until the next switch.

#### Scenario: Header during rotation
- **WHEN** the app is first of three and the next switch is 42 seconds away
- **THEN** the header shows the app name on the left and `[* o o] 42s` on the right

#### Scenario: Rotation held
- **WHEN** rotation is held by an urgent signal
- **THEN** the countdown is replaced with a hold marker

### Requirement: Indicator hidden without rotator
An app SHALL hide the rotation indicator when rotation state is missing or has not been updated for more than 5 seconds, and keep rendering the rest of the screen normally.

#### Scenario: Running standalone
- **WHEN** an app runs in a terminal with no rotator present
- **THEN** the header shows only the app name

#### Scenario: Rotator stalls
- **WHEN** rotation state stops updating for more than 5 seconds
- **THEN** the indicator disappears, and it returns when state updates resume

### Requirement: Target screen size
Every app SHALL lay out fully in a 60-column by 20-row terminal, and SHALL show a "terminal too small" message with the required and current size when the terminal is smaller.

#### Scenario: Minimum size
- **WHEN** the terminal is exactly 60x20
- **THEN** all panes render with no clipped or overlapping content

#### Scenario: Too small
- **WHEN** the terminal is resized below 60x20
- **THEN** the app shows the required and current size instead of its layout, and restores the layout when resized back

### Requirement: Consistent theme
All apps SHALL draw panes, titles, meters, graphs, and alert states using one shared theme so colors and border styles match across apps.

#### Scenario: Same styling across apps
- **WHEN** two different apps each display a titled pane and a meter
- **THEN** both use the same border style, title placement, and meter color gradient

### Requirement: Limited-color fallback
When the terminal does not support truecolor, apps SHALL render with a 16-color palette and characters available in standard console fonts, keeping all information legible.

#### Scenario: Linux console
- **WHEN** an app runs on a terminal that does not advertise truecolor support
- **THEN** graphs and meters render with block characters and the 16-color palette, and no characters show as missing-glyph boxes
