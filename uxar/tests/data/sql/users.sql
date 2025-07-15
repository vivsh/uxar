--@user_list
SELECT * FROM users;

--@user_by_id {"desc": "Get user by id"}
SELECT * FROM users WHERE id = $1;
