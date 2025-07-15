--@ping
SELECT 1;

--@complex_query {"meta": {"type": "complex"}}
SELECT * FROM foo WHERE bar = 'baz';
