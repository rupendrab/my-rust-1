@echo off

set tmpdir=c:\tmp
set s3_file=s3://i360-dev-political-dmi/API_CONTROL/processes.json
set AWS_PROFILE=dmi-s3-dev
set AWS_REGION=us-east-1
set RUST_LOG=info

target\debug\consumer-control-api.exe --port 3001
