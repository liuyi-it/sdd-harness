def shipping_fee(amount: int) -> int:
    """返回整数元运费，负数金额拒绝。"""
    if amount < 0:
        raise ValueError("订单金额不能为负数")
    return 0 if amount >= 100 else 10
