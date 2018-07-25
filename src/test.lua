function started()
  print("lua actor started")
end

function handle(msg)
  print('lua received', msg)
  return 420
end
