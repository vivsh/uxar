--@recent_orders
SELECT * FROM orders ORDER BY created_at DESC LIMIT 10;

--@order_by_id
SELECT * FROM orders WHERE id = $1;
