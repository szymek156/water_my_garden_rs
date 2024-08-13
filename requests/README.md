# Status
```bash
curl --insecure -X GET  http://192.168.68.57/status
```

# Ad-hoc watering of given section
Opens section immediately
```bash
curl --insecure -X POST -H "Content-Type: application/json" -d  @./requests/enable_section_for_req.json http://192.168.68.57/enable_section_for
```

# Enable watering
Schedules watering of all sections at some moment of day.
```bash
curl --insecure -X POST -H "Content-Type: application/json" -d  @./requests/start_watering_at_req.json http://192.168.68.57/start_watering_at
```

# Set section duration
Sets duration for given section, cannot be longer than 2 hours. Setting to 0 will skip the section.
```bash
curl --insecure -X POST -H "Content-Type: application/json" -d  @./requests/set_section_duration_req.json http://192.168.68.57/set_section_duration
```

# Disable watering
Disables alarm, sections durations are unaltered
```bash
curl --insecure -X POST -H "Content-Type: application/json" http://192.168.68.57/disable_watering
```

# Close valves
Closes GPIO, but does not touches alarms
```bash
curl --insecure -X POST -H "Content-Type: application/json" http://192.168.68.57/close_all_valves
```