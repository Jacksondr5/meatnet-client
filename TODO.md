- Research notes
  - It may be possible to read basic temperature data from the advertising packets alone, not neeting to take up a node space on the Meatnet. Otherwise, may need to connect to a Node so that other devices can connect directly to the probe. May be best to try and locate the repeater in any case.
  - It may be possible for me to fully manage the probe through the BLE connection. Need to consider if that's valuable now or as a later feature.
- Questions
  - How does the Meatnet heal itself? If one device loses connection to the probe, can it pick up with one of the other devices?

## TODO

- [ ] Get the BLE connection working in the CLI
  - [ ] Find the probe
  - [ ] Find the repeater
  - [ ] Log advertising packets
- [ ] Get the connection working in Rust
  - [ ] Find the probe
  - [ ] Find the repeater
  - [ ] Log advertising packets
  - [ ] Read other data from the probe status service
  - [ ] Use the UART service
