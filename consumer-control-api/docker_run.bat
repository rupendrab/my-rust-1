docker run ^
  -p 3002:80 ^
  --name consumer-control ^
  -e "s3_file=s3://i360-dev-political-dmi/API_CONTROL/processes.json" ^
  -e "tmpdir=/tmp" ^
  -e "AWS_ACCESS_KEY_ID=%aws_access_key_id%" ^
  -e "AWS_SECRET_ACCESS_KEY=%aws_secret_access_key%" ^
  -e "AWS_DEFAULT_REGION=us-east-1" ^
  -e "RUST_LOG=info" ^
  consumer-control-api
