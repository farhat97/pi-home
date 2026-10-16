# Spec Delta

## Purpose

Shows Pi-hole's activity and health at a glance on the device's small fullscreen terminal display, using data from the local Pi-hole v6 API.

## ADDED Requirements

### Requirement: Query history graph
The dashboard SHALL display a graph of DNS query volume over the last 24 hours, with blocked queries visually distinguished from permitted ones.

#### Scenario: History shown on start
- **WHEN** the dashboard starts and the Pi-hole API returns history data
- **THEN** a 24-hour query volume graph is shown, with the vertical scale labeled by its peak value

#### Scenario: Graph refreshes
- **WHEN** Pi-hole records new history intervals
- **THEN** the graph updates to include them without restarting the dashboard

### Requirement: Today's totals
The dashboard SHALL display today's total queries, blocked queries, the block percentage as a meter with a numeric value, and the number of active clients.

#### Scenario: Totals displayed
- **WHEN** the Pi-hole summary reports 48213 queries, 11902 blocked, and 17 clients
- **THEN** the dashboard shows those values and a block meter reading 24.7%

### Requirement: Top blocked domains
The dashboard SHALL list the most frequently blocked domains with their block counts, sorted by count descending, showing as many as fit in the available space.

#### Scenario: Domains listed
- **WHEN** the Pi-hole API returns a top-blocked list
- **THEN** the domains are shown in descending order of count, with counts right-aligned

#### Scenario: Long domain names
- **WHEN** a domain name is longer than the space available
- **THEN** the name is truncated with a visible marker and its count stays fully visible

### Requirement: Service status line
The dashboard SHALL show whether Pi-hole's DNS service (FTL) is running, whether blocking is active, the gravity list size, how long ago gravity was updated, and the Pi-hole version. Because Pi-hole v6 serves its API from FTL itself, FTL counts as down when the API cannot be reached.

#### Scenario: Healthy service
- **WHEN** FTL is running
- **THEN** the status line shows FTL as ok along with gravity size, gravity age, and version

#### Scenario: FTL down
- **WHEN** the API cannot be reached (connection refused or timed out)
- **THEN** the status line shows FTL as down in the theme's alert color

#### Scenario: Blocking not active
- **WHEN** the API reports blocking as disabled or failed
- **THEN** the status line shows the blocking state in the theme's alert color

### Requirement: Periodic refresh
The dashboard SHALL refresh summary and status data at a configurable interval, defaulting to 5 seconds or less, without user interaction.

#### Scenario: Values update
- **WHEN** query counts change in Pi-hole
- **THEN** the displayed totals reflect the change within one refresh interval

### Requirement: Authenticated API access
The dashboard SHALL authenticate to the Pi-hole v6 API using a password read from its configuration file and SHALL re-authenticate automatically when its session expires.

#### Scenario: Session expires
- **WHEN** the API rejects a request because the session expired
- **THEN** the dashboard re-authenticates and resumes updating without a restart

#### Scenario: Wrong password
- **WHEN** authentication fails because the configured password is wrong
- **THEN** the dashboard shows an authentication error on screen instead of metrics and keeps retrying at the refresh interval

#### Scenario: Config file permissions
- **WHEN** the configuration file holding the password is readable by users other than its owner
- **THEN** the dashboard logs a warning naming the file

### Requirement: Stale data when Pi-hole is unreachable
The dashboard SHALL stay running when the Pi-hole API is unreachable, keep showing the last known values marked as stale with their age, and resume normal display when the API returns.

#### Scenario: API goes away
- **WHEN** the API stops responding after data was previously loaded
- **THEN** the last values remain on screen with a stale marker and the time since the last successful update

#### Scenario: API unreachable at startup
- **WHEN** the dashboard starts and the API has never responded
- **THEN** the dashboard shows a waiting message and keeps retrying

#### Scenario: API recovers
- **WHEN** the API responds again
- **THEN** the stale marker disappears and values update normally

### Requirement: Urgent signal on Pi-hole failure
The dashboard SHALL raise an urgent attention signal while the API (and therefore FTL) has been unreachable for more than 30 seconds, renew it while the failure lasts, and SHALL clear it once the API responds again. Blocking being disabled or failed does not raise a signal.

#### Scenario: Failure raises signal
- **WHEN** the API has been unreachable for more than 30 seconds
- **THEN** the dashboard raises an urgent signal with a reason describing the failure

#### Scenario: Recovery clears signal
- **WHEN** the API is responding again
- **THEN** the dashboard clears its urgent signal
