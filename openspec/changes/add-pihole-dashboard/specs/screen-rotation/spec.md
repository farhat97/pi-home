# Spec Delta

## Purpose

Runs several fullscreen terminal apps on the device's single display and decides which one is visible, cycling on a schedule and giving way to apps that need attention.

## ADDED Requirements

### Requirement: Session starts at boot
The device SHALL start all configured apps, each fullscreen in its own window, on the physical display at boot without a login, with no visible status bar or terminal chrome.

#### Scenario: Boot to dashboard
- **WHEN** the Pi finishes booting
- **THEN** the display shows the first configured app fullscreen and no login prompt

### Requirement: Crashed apps restart
The system SHALL restart an app in its own window when it exits unexpectedly, without affecting the other apps.

#### Scenario: App crash
- **WHEN** one app process exits with an error
- **THEN** it is relaunched in the same window within a few seconds while other apps keep running

### Requirement: Scheduled rotation
The rotator SHALL show each configured app in turn for that app's configured dwell time, defaulting to 60 seconds, wrapping back to the first app after the last.

#### Scenario: Cycle through apps
- **WHEN** three apps are configured with 60-second dwell times
- **THEN** each app is shown for 60 seconds in configured order, then the cycle repeats

#### Scenario: Single app
- **WHEN** only one app is configured
- **THEN** that app stays on screen and no switching occurs

### Requirement: Rotation state is published
The rotator SHALL publish the active app's position, the total app count, the seconds until the next switch, and whether rotation is held, updated at least once per second.

#### Scenario: State readable by apps
- **WHEN** the rotator is running
- **THEN** any app can read the current rotation state, and each read returns a complete state (never partially written)

### Requirement: Urgent signals take over the screen
The rotator SHALL switch to an app that raises an urgent signal and hold it on screen while the signal is active, resuming the schedule after the signal clears or expires.

#### Scenario: Urgent signal raised
- **WHEN** an app that is not currently shown raises an urgent signal
- **THEN** the rotator switches to that app within 2 seconds and holds it there

#### Scenario: Signal cleared
- **WHEN** the urgent signal is cleared by the app
- **THEN** rotation resumes from the held app with a fresh dwell time

#### Scenario: Signal expires
- **WHEN** an urgent signal is older than its declared time-to-live and has not been renewed
- **THEN** the rotator treats it as cleared

#### Scenario: Several urgent signals
- **WHEN** more than one app has an active urgent signal
- **THEN** the rotator holds the one raised earliest, then moves to the next active one when it clears

### Requirement: Remote viewing
An operator SHALL be able to view the live display contents remotely over SSH without disturbing what is shown on the physical screen.

#### Scenario: Attach over SSH
- **WHEN** an operator connects over SSH and attaches to the display session
- **THEN** they see the same app the physical display is showing, and rotation continues

### Requirement: Rotator failure does not stop apps
Apps SHALL keep running and updating if the rotator stops, and the rotator SHALL be restarted automatically.

#### Scenario: Rotator crash
- **WHEN** the rotator process exits unexpectedly
- **THEN** the currently shown app keeps updating and the rotator is restarted
