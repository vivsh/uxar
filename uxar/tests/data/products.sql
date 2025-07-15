--@all_products
SELECT * FROM products;

--@product_by_sku {"desc": "Find by SKU"}
SELECT * FROM products WHERE sku = $1;
